import test from 'node:test';
import assert from 'node:assert/strict';
import { fixture, ready, writes } from './fixtures.mjs';

test('preparation is read-only; explicit confirmation writes all images then erases data', async () => {
  const f = fixture(); await ready(f);
  assert.deepEqual(writes(f), []);
  await f.session.install({ confirmErase: true });
  assert.deepEqual(writes(f), ['flash:system', 'flash:vendor', 'flash:dtbo', 'flash:vbmeta', 'flash:boot', 'erase']);
  assert.equal(f.session.phase, 'images-written');
  assert.ok(f.calls.indexOf('journal:flashing:system') < f.calls.indexOf('flash:system'));
  assert.equal(f.saved().phase, 'images-written');
  assert.equal(JSON.stringify(f.saved()).includes(f.device.serial), false);
  await f.session.reboot();
  assert.equal(f.session.phase, 'awaiting-boot');
  assert.equal(f.saved().phase, 'awaiting-boot');
  f.session.confirmBoot();
  assert.equal(f.session.phase, 'boot-confirmed-by-user');
  assert.equal(f.saved(), null);
});
test('install without erasure confirmation cannot write', async () => {
  const f = fixture(); await ready(f);
  await assert.rejects(f.session.install(), { code: 'erase-not-confirmed' });
  assert.deepEqual(writes(f), []);
});
test('later download or checksum failure produces no device writes', async () => {
  for (const reason of ['last image HTTP 404', 'last image checksum mismatch', 'disk full']) {
    const f = fixture({ prepare: async () => { throw new Error(reason); } });
    await f.session.connect();
    await assert.rejects(f.session.prepare());
    await assert.rejects(f.session.install({ confirmErase: true }));
    assert.deepEqual(writes(f), []);
  }
});
test('conflicting methods are rejected while download is pending', async () => {
  let finish;
  const f = fixture({ prepare: () => new Promise(resolve => { finish = resolve; }) });
  await f.session.connect();
  const pending = f.session.prepare();
  while (!finish) await new Promise(resolve => setImmediate(resolve));
  for (const task of [() => f.session.connect(), () => f.session.reboot(), () => f.session.unlock({ confirmErase: true }), () => f.session.install({ confirmErase: true })]) {
    await assert.rejects(task(), { code: 'busy' });
  }
  finish(f.prepared); await pending;
  assert.deepEqual(writes(f), []);
  assert.equal(f.session.phase, 'review');
});
test('phone swapped during download or before install blocks all writes', async () => {
  for (const when of ['during', 'after']) {
    const f = fixture();
    await f.session.connect();
    if (when === 'during') f.session.prepareRelease = async () => { f.device.serial = 'OTHER'; return f.prepared; };
    if (when === 'during') await assert.rejects(f.session.prepare(), { code: 'device-changed' });
    else { await f.session.prepare(); f.device.slot = 'a'; await assert.rejects(f.session.install({ confirmErase: true }), { code: 'device-changed' }); }
    assert.deepEqual(writes(f), []);
  }
});
test('partial write failure stops immediately, journals uncertainty and cannot be replayed', async () => {
  const f = fixture(); await ready(f);
  f.transport.flash = async image => { f.calls.push(`flash:${image.partition}`); throw new Error('USB disconnected mid-write'); };
  await assert.rejects(f.session.install({ confirmErase: true }));
  assert.deepEqual(writes(f), ['flash:system']);
  assert.equal(f.session.phase, 'recovery-required');
  assert.equal(f.saved().pending, 'system');
  assert.deepEqual(f.saved().written, []);
  await assert.rejects(f.session.install({ confirmErase: true }), { code: 'recovery-required' });
  await assert.rejects(f.session.reboot(), { code: 'restart-unavailable' });
  assert.deepEqual(writes(f), ['flash:system']);
});
test('slot changed between writes stops before the next partition', async () => {
  const f = fixture(); await ready(f);
  f.transport.flash = async image => { f.calls.push(`flash:${image.partition}`); f.device.slot = 'a'; };
  await assert.rejects(f.session.install({ confirmErase: true }), { code: 'device-changed' });
  assert.deepEqual(writes(f), ['flash:system']);
  assert.equal(f.session.phase, 'recovery-required');
});
test('journal storage failure before write prevents that write', async () => {
  const f = fixture(); await ready(f);
  f.journal.write = () => { throw new Error('storage unavailable'); };
  await assert.rejects(f.session.install({ confirmErase: true }));
  assert.deepEqual(writes(f), []);
});
test('unfinished journal after reload blocks a new install', async () => {
  const f = fixture({ saved: { phase: 'flashing', pending: 'system' } });
  await f.session.connect();
  await assert.rejects(f.session.prepare(), { code: 'recovery-required' });
  assert.deepEqual(writes(f), []);
});
test('a prepared tab re-reads another tab’s write journal before installation', async () => {
  const f = fixture(); await ready(f);
  f.journal.write({ phase: 'flashing', pending: 'system' });
  await assert.rejects(f.session.install({ confirmErase: true }), { code: 'recovery-required' });
  assert.deepEqual(writes(f), []);
});
test('public origin remains read-only even for an otherwise valid phone and release', async () => {
  const f = fixture({ allowWrites: false }); await f.session.connect();
  await assert.rejects(f.session.prepare(), { code: 'preview-read-only' });
  f.device.unlocked = 'no';
  await assert.rejects(f.session.unlock({ confirmErase: true }), { code: 'preview-read-only' });
  assert.deepEqual(writes(f), []);
});
test('unsupported phone cannot be unlocked through the controller', async () => {
  const f = fixture(); f.device.codename = 'fajita'; f.device.unlocked = 'no';
  await f.session.connect();
  await assert.rejects(f.session.unlock({ confirmErase: true }), { code: 'model-unverified' });
  assert.deepEqual(writes(f), []);
});
test('unlock needs explicit erasure confirmation and requires reconnect afterward', async () => {
  const f = fixture(); f.device.unlocked = 'no'; await f.session.connect();
  await assert.rejects(f.session.unlock(), { code: 'erase-not-confirmed' });
  await f.session.unlock({ confirmErase: true });
  assert.deepEqual(writes(f), ['unlock']);
  assert.equal(f.session.target, null);
  assert.equal(f.session.phase, 'unlock-requested');
});
