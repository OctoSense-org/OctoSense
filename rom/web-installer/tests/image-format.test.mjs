import test from 'node:test';
import assert from 'node:assert/strict';
import { validateImageFormat } from '../src/image-format.mjs';

function sparse() {
  const bytes = new Uint8Array(28 + 12 + 4096 + 12 + 4 + 12 + 12 + 4);
  const view = new DataView(bytes.buffer);
  view.setUint32(0, 0xed26ff3a, true); view.setUint16(4, 1, true);
  view.setUint16(8, 28, true); view.setUint16(10, 12, true);
  view.setUint32(12, 4096, true); view.setUint32(16, 3, true); view.setUint32(20, 4, true);
  let offset = 28;
  for (const [type, blocks, size] of [[0xcac1, 1, 4108], [0xcac2, 1, 16], [0xcac3, 1, 12], [0xcac4, 0, 16]]) {
    view.setUint16(offset, type, true); view.setUint32(offset + 4, blocks, true); view.setUint32(offset + 8, size, true); offset += size;
  }
  return { bytes, view, image: { partition: 'system', size: bytes.length, expanded_size: 12288, sparse: true } };
}
test('sparse framing validates raw, fill, skip and CRC chunks without loading payloads', async () => {
  const { bytes, image } = sparse(); await validateImageFormat(image, new Blob([bytes]));
});
for (const [name, change] of [
  ['incorrect expansion', f => f.image.expanded_size++],
  ['format metadata mismatch', f => f.image.sparse = false],
  ['truncated chunk table', f => f.view.setUint32(20, 10, true)],
  ['unknown chunk type', f => f.view.setUint16(28, 0x1234, true)],
  ['invalid raw chunk length', f => f.view.setUint32(36, 4109, true)],
  ['block count overflow', f => f.view.setUint32(32, 4, true)],
  ['unsupported header extension', f => f.view.setUint16(8, 32, true)],
]) test(`sparse framing rejects ${name} before flashing`, async () => {
  const f = sparse(); change(f);
  await assert.rejects(validateImageFormat(f.image, new Blob([f.bytes])), { code: 'image-format' });
});
