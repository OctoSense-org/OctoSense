import { createHash } from 'node:crypto';
import { PARTITIONS, RECIPE, validateManifest } from '../src/contracts.mjs';
import { InstallSession } from '../src/session.mjs';

export const bytes = new TextEncoder().encode('verified fixture image');
export const digest = input => createHash('sha256').update(input).digest('hex');
export function manifestInput() {
  return { schema: 1, name: 'Test release', built: '2026-09-23T00:00:00Z', device: 'enchilada', recipe: RECIPE,
    channel: 'development', images: PARTITIONS.map(partition => ({ partition, file: `${partition}.img`,
      size: bytes.length, expanded_size: bytes.length, sparse: false, sha256: digest(bytes), slot: partition !== 'vbmeta' })) };
}
export const release = () => validateManifest(manifestInput(), 'http://localhost/manifest.json');
export function target() {
  return { product: 'sdm845', codename: 'enchilada', serial: 'SIMULATED-ONLY', slot: 'b', unlocked: 'yes',
    userspace: 'no', maxDownload: 64 * 1024 ** 2,
    partitions: Object.fromEntries(PARTITIONS.map(partition => [partition, { size: 8 * 1024 ** 3, slotted: partition !== 'vbmeta' }])) };
}
export function fixture(options = {}) {
  const calls = [];
  let saved = options.saved ?? null;
  const device = target();
  const journal = { read: () => saved, write: value => { calls.push(`journal:${value.phase}:${value.pending ?? '-'}`); saved = structuredClone(value); }, clear: () => { saved = null; } };
  const transport = {
    connect: async () => { calls.push('connect'); }, inspect: async () => structuredClone(device),
    flash: async image => { calls.push(`flash:${image.partition}`); }, eraseData: async () => { calls.push('erase'); },
    unlock: async () => { calls.push('unlock'); }, reboot: async () => { calls.push('reboot'); },
  };
  const prepared = { files: new Map(PARTITIONS.map(partition => [partition, new Blob([bytes])])), dispose: async () => { calls.push('dispose'); } };
  const session = new InstallSession({ transport, journal, prepare: async () => prepared, exclusive: async task => task(), allowWrites: true, ...options });
  session.setManifest(release());
  return { session, device, transport, journal, prepared, calls, saved: () => saved };
}
export async function ready(f) { await f.session.connect(); await f.session.prepare(); }
export const writes = f => f.calls.filter(call => /^(flash:|erase$|unlock$|reboot$)/.test(call));
