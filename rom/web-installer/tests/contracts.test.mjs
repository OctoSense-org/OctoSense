import test from 'node:test';
import assert from 'node:assert/strict';
import { validateManifest, loadManifest, assertTarget, localWritePolicy } from '../src/contracts.mjs';
import { manifestInput, release, target } from './fixtures.mjs';

const invalid = error => error.code === 'manifest-invalid';
test('complete manifest becomes an immutable allowlisted release', () => {
  const input = manifestInput();
  const parsed = validateManifest(input, 'https://example.test/build/manifest.json');
  input.images[0].file = 'changed.img';
  assert.equal(parsed.images[0].file, 'system.img');
  assert.ok(Object.isFrozen(parsed.images[0]));
  assert.match(parsed.images[0].url, /^https:\/\/example.test\/build\/system.img\?sha256=/);
});
for (const [name, mutate] of [
  ['legacy schema', m => delete m.schema], ['empty images', m => m.images = []],
  ['missing image', m => m.images.pop()], ['duplicate image', m => m.images[1] = m.images[0]],
  ['unsupported device', m => m.device = 'fajita'], ['shared chipset identity', m => m.device = 'sdm845'],
  ['unsigned stable channel', m => m.channel = 'stable'], ['unknown field', m => m.force = true],
  ['arbitrary partition', m => m.images[0].partition = 'userdata'], ['path traversal', m => m.images[0].file = '../system.img'],
  ['cross-origin image', m => m.images[0].file = 'https://evil.test/system.img'],
  ['invalid hash', m => m.images[0].sha256 = 'bad'], ['zero size', m => m.images[0].size = 0],
  ['overflowing size', m => m.images[0].size = Number.MAX_SAFE_INTEGER],
  ['raw expanded size mismatch', m => m.images[0].expanded_size++],
  ['unsafe vbmeta slot', m => m.images.find(i => i.partition === 'vbmeta').slot = true],
]) test(`manifest rejects ${name}`, () => {
  const input = manifestInput(); mutate(input);
  assert.throws(() => validateManifest(input, 'http://localhost/manifest.json'), invalid);
});
test('HTTP failure is checked before reading manifest JSON', async () => {
  let read = false;
  await assert.rejects(loadManifest('http://localhost/manifest.json', async () => ({ ok: false, status: 404, text: () => { read = true; } })), { code: 'manifest-http' });
  assert.equal(read, false);
});
for (const [name, mutate, code] of [
  ['unknown model on shared chipset', t => t.codename = null, 'model-unverified'],
  ['6T', t => t.codename = 'fajita', 'model-unverified'],
  ['contradictory model identity', t => { t.product = 'enchilada'; t.codename = 'fajita'; }, 'model-unverified'],
  ['locked', t => t.unlocked = 'no', 'bootloader-locked'],
  ['unknown unlock state', t => t.unlocked = null, 'unlock-unknown'],
  ['fastbootd', t => t.userspace = 'yes', 'mode-unverified'],
  ['unknown slot', t => t.slot = '?', 'slot-unknown'],
  ['missing transfer capability', t => t.maxDownload = null, 'transfer-limit-unknown'],
  ['undersized partition', t => t.partitions.system.size = 1, 'partition-mismatch'],
  ['incorrect partition layout', t => t.partitions.system.slotted = false, 'partition-mismatch'],
]) test(`device check blocks ${name}`, () => {
  const current = target(); mutate(current);
  assert.throws(() => assertTarget(current, release()), { code });
});
test('reconnect checks serial and slot against the original target', () => {
  for (const change of [{ serial: 'OTHER-PHONE' }, { slot: 'a' }]) {
    assert.throws(() => assertTarget({ ...target(), ...change }, release(), target()), { code: 'device-changed' });
  }
});
test('unsigned development writes are limited to loopback origins', () => {
  for (const url of ['http://localhost:8321', 'http://127.0.0.1:8321', 'http://[::1]:8321']) assert.equal(localWritePolicy(new URL(url)), true);
  for (const url of ['https://example.test', 'https://localhost.evil.test', 'file:///tmp/test.html', 'http://192.168.1.2']) assert.equal(localWritePolicy(new URL(url)), false);
});
test('an image that cannot fit the transfer limit blocks the plan before writing', () => {
  for (const partition of ['boot', 'vbmeta']) {
    const input = manifestInput();
    Object.assign(input.images.find(image => image.partition === partition), { size: 60 * 1024 ** 2, expanded_size: 60 * 1024 ** 2 });
    assert.throws(() => assertTarget(target(), validateManifest(input, 'http://localhost/manifest.json')), { code: 'transfer-limit' });
  }
});
