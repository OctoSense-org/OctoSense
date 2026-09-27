import { requireCondition } from './contracts.mjs';

/** Validate sparse framing before any flash; reads only fixed-size headers. */
export async function validateImageFormat(image, file) {
  const check = condition => requireCondition(condition, 'image-format', `${image.partition}: image format does not match the release metadata or supported sparse format.`);
  check(file.size === image.size);
  const header = new DataView(await file.slice(0, 28).arrayBuffer());
  const sparse = header.byteLength >= 4 && header.getUint32(0, true) === 0xed26ff3a;
  check(sparse === image.sparse);
  if (!sparse) { check(image.expanded_size === file.size); return; }
  check(header.byteLength === 28);
  check(header.getUint16(4, true) === 1 && header.getUint16(8, true) === 28 && header.getUint16(10, true) === 12);
  const blockSize = header.getUint32(12, true);
  const totalBlocks = header.getUint32(16, true);
  const chunks = header.getUint32(20, true);
  check(blockSize > 0 && blockSize % 4 === 0 && totalBlocks > 0 && chunks > 0);
  check(blockSize * totalBlocks === image.expanded_size && chunks <= (file.size - 28) / 12);
  let offset = 28, blocks = 0;
  for (let index = 0; index < chunks; index++) {
    const chunk = new DataView(await file.slice(offset, offset + 12).arrayBuffer());
    check(chunk.byteLength === 12);
    const type = chunk.getUint16(0, true), count = chunk.getUint32(4, true), size = chunk.getUint32(8, true);
    const payload = type === 0xcac1 ? count * blockSize : [0xcac2, 0xcac4].includes(type) ? 4 : type === 0xcac3 ? 0 : null;
    check(payload !== null && size === 12 + payload && offset + size <= file.size);
    check(type !== 0xcac4 || count === 0);
    blocks += count; offset += size;
    check(blocks <= totalBlocks);
  }
  check(offset === file.size && blocks === totalBlocks);
}
