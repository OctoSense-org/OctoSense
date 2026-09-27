import test from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { verifyImageStream, requireStorage } from '../src/download.mjs';
import { bytes, digest } from './fixtures.mjs';

function sink() { return { bytes: 0, closed: false, aborted: false, async write(value) { this.bytes += value.length; }, async close() { this.closed = true; }, async abort() { this.aborted = true; } }; }
const image = input => ({ partition: 'boot', size: input.length, sha256: digest(input) });
test('SHA-256 matches Node crypto across block and stream boundaries', async () => {
  for (const size of [1, 55, 56, 63, 64, 65, 127, 128, 1024 * 1024 + 17]) {
    const input = new Uint8Array(size).map((_, i) => i % 251);
    let offset = 0;
    const response = new Response(new ReadableStream({ pull(controller) {
      if (offset === size) controller.close();
      else { const end = Math.min(size, offset + 777); controller.enqueue(input.slice(offset, end)); offset = end; }
    } }));
    const output = sink();
    assert.equal((await verifyImageStream(image(input), response, output)).sha256, digest(input));
    assert.equal(output.closed, true);
    assert.equal(output.bytes, size);
  }
});
test('every byte of a 64 MiB image is hashed, including its final chunk', async () => {
  const chunk = new Uint8Array(1024 * 1024).fill(9);
  const hash = createHash('sha256');
  for (let i = 0; i < 64; i++) hash.update(chunk);
  const expected = hash.digest('hex');
  for (const corrupt of [false, true]) {
    let count = 0;
    const response = new Response(new ReadableStream({ pull(controller) {
      if (count === 64) return controller.close();
      const part = chunk.slice();
      if (++count === 64 && corrupt) part[part.length - 1] ^= 1;
      controller.enqueue(part);
    } }));
    const output = sink();
    const task = verifyImageStream({ partition: 'boot', size: 64 * chunk.length, sha256: expected }, response, output);
    if (corrupt) { await assert.rejects(task, { code: 'image-checksum' }); assert.equal(output.aborted, true); assert.equal(output.closed, false); }
    else { await task; assert.equal(output.closed, true); }
  }
});
for (const [name, metadata, response, code] of [
  ['small corrupt image', { ...image(bytes), sha256: '0'.repeat(64) }, () => new Response(bytes), 'image-checksum'],
  ['short body', { ...image(bytes), size: bytes.length + 1 }, () => new Response(bytes), 'image-size'],
  ['long body', { ...image(bytes), size: bytes.length - 1 }, () => new Response(bytes), 'image-size'],
  ['HTTP failure', image(bytes), () => new Response('', { status: 404 }), 'image-http'],
]) test(`${name} aborts its temporary file`, async () => {
  const output = sink();
  await assert.rejects(verifyImageStream(metadata, response(), output), { code });
  assert.equal(output.aborted, true); assert.equal(output.closed, false);
});
test('network interruption and disk failure do not complete verification', async () => {
  const broken = new Response(new ReadableStream({ start(controller) { controller.error(new Error('connection lost')); } }));
  const output = sink();
  await assert.rejects(verifyImageStream(image(bytes), broken, output));
  assert.equal(output.aborted, true);
  const full = sink(); full.write = async () => { throw new Error('disk full'); };
  await assert.rejects(verifyImageStream(image(bytes), new Response(bytes), full));
  assert.equal(full.aborted, true);
});
test('storage capability and space are checked before staging', async () => {
  await assert.rejects(requireStorage({}, 100), { code: 'storage-unavailable' });
  await assert.rejects(requireStorage({ getDirectory() {}, estimate: async () => ({ quota: 100, usage: 90 }) }, 100), { code: 'storage-full' });
});
