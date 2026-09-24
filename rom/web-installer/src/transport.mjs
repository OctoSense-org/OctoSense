import { FastbootDevice, FastbootError, setDebugLevel } from '../fastboot.mjs';
import { InstallerError, PARTITIONS, TRANSFER_CAP, requireCondition } from './contracts.mjs';

const filters = [{ classCode: 0xff, subclassCode: 0x42, protocolCode: 0x03 }];
const sizeValue = value => typeof value === 'string' && /^(?:0x)?[a-f0-9]+$/i.test(value) ? Number.parseInt(value, 16) : null;

async function deadline(operation, ms, onTimeout) {
  let timer;
  try {
    return await Promise.race([operation(), new Promise((_, reject) => {
      timer = setTimeout(() => {
        onTimeout();
        reject(new InstallerError('usb-timeout', 'The phone stopped responding. The operation will not be replayed automatically.'));
      }, ms);
    })]);
  } finally { clearTimeout(timer); }
}

/** Adapt the vendored library without its automatic device-replacement listeners. */
export class BoundFastbootDevice extends FastbootDevice {
  constructor(usb = navigator.usb) {
    super();
    this.usb = usb;
    this.valid = false;
    this.bound = null;
    this.permit = null;
    usb?.addEventListener('disconnect', event => {
      if (event.device === this.bound) this.invalidate();
    });
  }

  invalidate() {
    this.valid = false;
    this.permit = null;
    if (this.bound?.opened) void this.bound.close().catch(() => {});
  }

  async connect() {
    this.valid = false;
    this.permit = null;
    if (this.bound?.opened) await this.bound.close().catch(() => {});
    const paired = await this.usb.getDevices();
    const candidates = paired.filter(device => device.configurations.some(config => config.interfaces.some(iface =>
      iface.alternates.some(alt => alt.interfaceClass === 0xff && alt.interfaceSubclass === 0x42 && alt.interfaceProtocol === 0x03))));
    const selected = candidates.length === 1 ? candidates[0] : await this.usb.requestDevice({ filters });
    this.device = selected;
    this.bound = selected;
    await this._validateAndConnectDevice();
    this.valid = true;
  }

  guard(command) {
    requireCondition(this.valid && this.device === this.bound && this.bound?.opened, 'usb-disconnected', 'The selected phone disconnected. Reconnect it and review the device state.');
    if (command.startsWith('getvar:')) return;
    const permit = this.permit;
    const download = /^download:([a-f0-9]{8})$/.exec(command);
    const allowed = (permit?.kind === 'flash' &&
      (command === `flash:${permit.partition}` || (download && Number.parseInt(download[1], 16) <= permit.maxDownload))) ||
      (permit?.kind === 'erase' && command === 'erase:userdata') ||
      (permit?.kind === 'unlock' && command === 'flashing unlock') ||
      (permit?.kind === 'reboot' && command === 'reboot');
    requireCondition(allowed, 'usb-command-denied', 'The device command is outside the reviewed operation.');
  }

  async runCommand(command) {
    this.guard(command);
    const timeout = command.startsWith('getvar:') ? 10_000 : 120_000;
    try { return await deadline(() => super.runCommand(command), timeout, () => this.invalidate()); }
    catch (error) {
      if (!(error instanceof FastbootError && error.status === 'FAIL')) this.invalidate();
      throw error;
    }
  }

  async getVariable(name) {
    // The library's separate getvar timer does not close a timed-out connection.
    // Keep one deadline so late replies cannot be consumed by a later command.
    try { return (await this.runCommand(`getvar:${name}`)).text?.trim() || null; }
    catch (error) {
      if (error instanceof FastbootError && error.status === 'FAIL') return null;
      throw error;
    }
  }

  async _getDownloadSize() {
    const size = sizeValue(await this.getVariable('max-download-size'));
    requireCondition(Number.isSafeInteger(size) && size >= 1024 * 1024, 'transfer-limit-unknown', 'The phone transfer limit is unknown.');
    return Math.min(size, TRANSFER_CAP);
  }
}

export class FastbootTransport {
  constructor(driver = new BoundFastbootDevice()) {
    this.driver = driver;
    setDebugLevel(0);
  }

  connect() { return this.driver.connect(); }

  async inspect() {
    const read = name => this.driver.getVariable(name);
    const target = {};
    for (const [key, variable] of Object.entries({ product: 'product', codename: 'device', serial: 'serialno',
      slot: 'current-slot', unlocked: 'unlocked', userspace: 'is-userspace' })) target[key] = await read(variable);
    if (this.driver.bound?.serialNumber) requireCondition(target.serial === this.driver.bound.serialNumber,
      'identity-mismatch', 'The USB and bootloader identities do not match.');
    target.maxDownload = sizeValue(await read('max-download-size'));
    target.partitions = {};
    for (const partition of PARTITIONS) {
      const slotValue = await read(`has-slot:${partition}`);
      // The known OnePlus 6 vbmeta quirk requires its unsuffixed partition.
      const slotted = partition === 'vbmeta' ? false : slotValue === 'yes' ? true : slotValue === 'no' ? false : null;
      const name = slotted === true ? `${partition}_${target.slot}` : partition;
      target.partitions[partition] = { slotted, size: sizeValue(await read(`partition-size:${name}`)) };
    }
    return target;
  }

  async permitted(permit, task) {
    requireCondition(!this.driver.permit, 'busy', 'A USB operation is already in progress.');
    this.driver.permit = permit;
    try { return await deadline(task, 15 * 60_000, () => this.driver.invalidate()); }
    finally { this.driver.permit = null; }
  }

  async flash(image, file, target, onProgress) {
    const partition = image.slot ? `${image.partition}_${target.slot}` : image.partition;
    const maxDownload = Math.min(target.maxDownload, TRANSFER_CAP);
    return this.permitted({ kind: 'flash', partition, maxDownload }, async () => {
      if (image.slot) {
        // Explicit physical partition plus command allowlisting prevents slot drift.
        await this.driver.flashBlob(partition, file, onProgress);
      } else {
        requireCondition(file.size <= maxDownload, 'transfer-limit', 'The unslotted image exceeds the transfer limit.');
        await this.driver.upload(partition, await file.arrayBuffer(), onProgress);
        await this.driver.runCommand(`flash:${partition}`);
      }
    });
  }

  eraseData() { return this.permitted({ kind: 'erase' }, () => this.driver.runCommand('erase:userdata')); }
  unlock() { return this.permitted({ kind: 'unlock' }, () => this.driver.runCommand('flashing unlock')); }
  reboot() { return this.permitted({ kind: 'reboot' }, () => this.driver.reboot('')); }
}
