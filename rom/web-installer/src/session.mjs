import { InstallerError, assertIdentity, assertTarget, requireCondition } from './contracts.mjs';

/** The controller is independent of DOM, USB and storage implementations. */
export class InstallSession {
  constructor({ transport, prepare, journal, exclusive, allowWrites = false, onChange = () => {} }) {
    Object.assign(this, { transport, prepareRelease: prepare, journal, exclusive, allowWrites, onChange });
    this.manifest = null;
    this.target = null;
    this.prepared = null;
    this.plan = null;
    this.busy = false;
    this.dirty = false;
    this.written = [];
    this.phase = 'disconnected';
    this.error = null;
    try { this.pendingJournal = journal.read(); }
    catch { this.pendingJournal = { phase: 'journal-unreadable' }; }
    if (this.pendingJournal) this.phase = 'recovery-required';
  }

  snapshot() {
    return { phase: this.phase, busy: this.busy, error: this.error, progress: this.progress,
      target: this.target && structuredClone(this.target), manifest: this.manifest,
      written: [...this.written], recoveryRequired: Boolean(this.pendingJournal) };
  }

  notify() { this.onChange(this.snapshot()); }

  setManifest(manifest) {
    requireCondition(!this.busy && !this.prepared, 'busy', 'Finish the current operation before changing releases.');
    this.manifest = manifest;
    this.notify();
  }

  assertWriteAccess() {
    requireCondition(this.allowWrites, 'preview-read-only', 'Public flashing is disabled until signed releases and device qualification are ready. Use the local developer preview for testing.');
    requireCondition(!this.pendingJournal, 'recovery-required', 'An earlier installation needs recovery review. Ordinary installation is blocked.');
    requireCondition(this.manifest, 'release-missing', 'Load a valid release before continuing.');
  }

  async run(operation) {
    requireCondition(!this.busy, 'busy', 'Another device operation is in progress.');
    this.busy = true;
    this.error = null;
    this.notify();
    try {
      return await this.exclusive(async () => {
        // Another tab may have written after this controller was constructed.
        // Re-read under the origin lock before checking any write preconditions.
        try { this.pendingJournal = this.journal.read(); }
        catch {
          this.pendingJournal = { phase: 'journal-unreadable' };
          throw new InstallerError('journal-unreadable', 'The installation journal is unavailable. Device writes are blocked.');
        }
        return operation();
      });
    }
    catch (error) {
      this.phase = this.dirty || this.pendingJournal ? 'recovery-required' : 'blocked';
      this.error = { code: error.code ?? 'operation-failed', message: error.message };
      if (this.dirty) {
        try { this.record('recovery-required', this.pendingStep); } catch { /* The earlier write intent remains. */ }
      }
      throw error;
    } finally {
      this.busy = false;
      this.progress = null;
      this.notify();
    }
  }

  async discardPrepared() {
    const prepared = this.prepared;
    this.prepared = null;
    this.plan = null;
    await prepared?.dispose().catch(() => {});
  }

  async connect() {
    return this.run(async () => {
      await this.discardPrepared();
      this.target = null;
      if (!this.pendingJournal) { this.written = []; this.dirty = false; }
      this.phase = 'connecting'; this.notify();
      await this.transport.connect();
      this.target = await this.transport.inspect();
      this.phase = this.pendingJournal ? 'recovery-required' : 'connected';
    });
  }

  async prepare() {
    return this.run(async () => {
      this.assertWriteAccess();
      requireCondition(this.target, 'device-missing', 'Connect the phone first.');
      await this.discardPrepared();
      const expected = assertTarget(await this.transport.inspect(), this.manifest, this.target);
      this.phase = 'preparing'; this.notify();
      let prepared;
      try {
        prepared = await this.prepareRelease(this.manifest, progress => {
          this.progress = progress; this.notify();
        });
        assertTarget(await this.transport.inspect(), this.manifest, expected);
        requireCondition(prepared.files.size === this.manifest.images.length, 'staging-incomplete', 'The complete release was not staged.');
        for (const image of this.manifest.images) {
          requireCondition(prepared.files.get(image.partition)?.size === image.size, 'staging-incomplete', `${image.partition}: verified image is unavailable.`);
        }
        this.prepared = prepared;
        this.plan = { manifest: this.manifest, target: expected };
        this.phase = 'review';
      } catch (error) {
        await prepared?.dispose().catch(() => {});
        throw error;
      }
    });
  }

  record(phase, pending = null) {
    const record = { schema: 1, operation: this.operationId, phase, release: this.plan.manifest.name,
      deviceFingerprint: this.deviceFingerprint, slot: this.plan.target.slot,
      written: [...this.written], pending, updated: new Date().toISOString() };
    this.journal.write(record); // Persist intent before a device write, or fail without that write.
    this.pendingJournal = record;
    this.pendingStep = pending;
  }

  async install({ confirmErase = false } = {}) {
    return this.run(async () => {
      this.assertWriteAccess();
      requireCondition(this.phase === 'review' && this.prepared && this.plan, 'not-prepared', 'Download and verify the complete release first.');
      requireCondition(confirmErase === true, 'erase-not-confirmed', 'Confirm that this fresh installation will erase all phone data.');
      const { manifest, target } = this.plan;
      assertTarget(await this.transport.inspect(), manifest, target);
      this.operationId = crypto.randomUUID();
      const identity = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(target.serial));
      this.deviceFingerprint = Array.from(new Uint8Array(identity), byte => byte.toString(16).padStart(2, '0')).join('');
      this.written = [];
      this.phase = 'flashing'; this.notify();
      for (const image of manifest.images) {
        assertTarget(await this.transport.inspect(), manifest, target);
        this.record('flashing', image.partition);
        this.dirty = true; // A thrown transport error does not prove that nothing was written.
        await this.transport.flash(image, this.prepared.files.get(image.partition), target, value => {
          this.progress = { type: 'flash', partition: image.partition, value }; this.notify();
        });
        this.written.push(image.partition);
        this.record('flashing');
      }
      assertTarget(await this.transport.inspect(), manifest, target);
      this.record('flashing', 'userdata');
      await this.transport.eraseData(target);
      this.written.push('userdata');
      this.record('images-written');
      this.phase = 'images-written';
      // Keep the plan for restart verification; remove only the staged images.
      await this.prepared.dispose().catch(() => {});
      this.prepared = null;
    });
  }

  async unlock({ confirmErase = false } = {}) {
    return this.run(async () => {
      this.assertWriteAccess();
      requireCondition(this.target, 'device-missing', 'Connect the phone first.');
      const target = assertTarget(await this.transport.inspect(), this.manifest, this.target, false);
      requireCondition(target.unlocked === 'no', 'unlock-state', 'The bootloader is not confirmed locked.');
      requireCondition(confirmErase === true, 'erase-not-confirmed', 'Confirm that unlocking erases all phone data.');
      await this.discardPrepared();
      await this.transport.unlock(target);
      this.target = null;
      this.phase = 'unlock-requested';
    });
  }

  async reboot() {
    return this.run(async () => {
      requireCondition(this.allowWrites && this.phase === 'images-written' && this.plan, 'restart-unavailable', 'Restart is available after all images and data formatting finish.');
      assertTarget(await this.transport.inspect(), this.plan.manifest, this.plan.target);
      this.record('restart-requested');
      await this.transport.reboot(this.plan.target);
      this.phase = 'awaiting-boot';
      this.target = null;
      this.record('awaiting-boot');
    });
  }

  confirmBoot() {
    requireCondition(!this.busy && this.phase === 'awaiting-boot', 'boot-not-requested', 'Wait until the phone has restarted.');
    this.journal.clear();
    this.pendingJournal = null;
    this.dirty = false;
    this.phase = 'boot-confirmed-by-user';
    this.notify();
  }

  compatibility() {
    try {
      this.assertWriteAccess();
      assertIdentity(this.target);
      assertTarget(this.target, this.manifest, null, false);
      return null;
    } catch (error) { return error.message; }
  }
}

export const browserJournal = {
  read() { const value = localStorage.getItem('octosense-install-journal-v1'); return value === null ? null : JSON.parse(value); },
  write(value) { localStorage.setItem('octosense-install-journal-v1', JSON.stringify(value)); },
  clear() { localStorage.removeItem('octosense-install-journal-v1'); },
};

export async function browserExclusive(operation) {
  requireCondition(navigator.locks, 'locks-unavailable', 'This browser cannot reserve exclusive device access.');
  return navigator.locks.request('octosense-installer-usb', { ifAvailable: true }, lock => {
    if (!lock) throw new InstallerError('device-busy', 'Another OctoSense installer tab is using the device.');
    return operation();
  });
}
