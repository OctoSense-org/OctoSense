import test from 'node:test';
import assert from 'node:assert/strict';
import { BoundFastbootDevice, FastbootTransport } from '../src/transport.mjs';

function usbFixture() {
  const usb = new EventTarget();
  const commands = [];
  const phone = {
    opened: false, serialNumber: 'SIMULATED-USB',
    configurations: [{ interfaces: [{ claimed: false, alternates: [{ interfaceClass: 0xff, interfaceSubclass: 0x42, interfaceProtocol: 3,
      endpoints: [{ type: 'bulk', direction: 'in', endpointNumber: 1 }, { type: 'bulk', direction: 'out', endpointNumber: 2 }] }] }] }],
    async open() { this.opened = true; }, async close() { this.opened = false; }, async reset() {},
    async selectConfiguration() {}, async claimInterface() { this.configurations[0].interfaces[0].claimed = true; },
    async transferOut(_, packet) { commands.push(new TextDecoder().decode(packet)); return { status: 'ok', bytesWritten: packet.byteLength }; },
    async transferIn() {
      const reply = commands.at(-1) === 'getvar:unsupported' ? 'FAILunsupported' : 'OKAYenchilada';
      return { status: 'ok', data: new DataView(new TextEncoder().encode(reply).buffer) };
    },
  };
  usb.getDevices = async () => [phone];
  usb.requestDevice = async () => { throw new Error('unexpected picker'); };
  return { usb, phone, commands, driver: new BoundFastbootDevice(usb) };
}

test('real adapter rejects unreviewed commands before USB transfer', async () => {
  const f = usbFixture(); await f.driver.connect();
  assert.equal(await f.driver.getVariable('product'), 'enchilada');
  for (const command of ['reboot', 'erase:userdata', 'flashing unlock', 'flash:system_b']) {
    await assert.rejects(f.driver.runCommand(command), { code: 'usb-command-denied' });
  }
  assert.deepEqual(f.commands, ['getvar:product']);
});
test('flash permit binds both the physical partition and transfer limit', async () => {
  const f = usbFixture(); await f.driver.connect();
  f.driver.permit = { kind: 'flash', partition: 'boot_b', maxDownload: 1024 };
  await f.driver.runCommand('flash:boot_b');
  for (const command of ['flash:boot_a', 'erase:userdata', 'resize-logical-partition:boot_b:0', 'download:00000800']) {
    await assert.rejects(f.driver.runCommand(command), { code: 'usb-command-denied' });
  }
  assert.deepEqual(f.commands, ['flash:boot_b']);
});
test('an attached second phone is never automatically selected', async () => {
  const f = usbFixture(); await f.driver.connect();
  const other = { ...f.phone, serialNumber: 'OTHER', opened: false };
  const connect = new Event('connect'); connect.device = other; f.usb.dispatchEvent(connect);
  assert.equal(f.driver.bound, f.phone);
  assert.equal(f.driver.device, f.phone);
  assert.equal(await f.driver.getVariable('product'), 'enchilada');
  const disconnect = new Event('disconnect'); disconnect.device = f.phone; f.usb.dispatchEvent(disconnect);
  f.usb.dispatchEvent(connect);
  await assert.rejects(f.driver.getVariable('product'), { code: 'usb-disconnected' });
  assert.deepEqual(f.commands, ['getvar:product']);
});
test('unsupported getvar is normalized without consuming a later reply', async () => {
  const f = usbFixture(); await f.driver.connect();
  assert.equal(await f.driver.getVariable('unsupported'), null);
  assert.equal(await f.driver.getVariable('product'), 'enchilada');
});
test('transport flashes the frozen physical slot and does not retry a failure', async () => {
  const calls = [];
  const driver = { permit: null, invalidate() {}, async flashBlob(partition) { calls.push(partition); throw new Error('link lost'); } };
  const transport = new FastbootTransport(driver);
  await assert.rejects(transport.flash({ partition: 'boot', slot: true }, new Blob(['image']), { slot: 'b', maxDownload: 64 * 1024 ** 2 }, () => {}));
  assert.deepEqual(calls, ['boot_b']);
  assert.equal(driver.permit, null);
});
