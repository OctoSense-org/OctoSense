import { sha256 } from '../vendor/noble-hashes/sha2.js';
import { InstallerError, requireCondition } from './contracts.mjs';

/** Hash every received byte and write bounded chunks to a temporary sink. */
export async function verifyImageStream(image, response, sink, onProgress = () => {}) {
  let reader;
  const hash = sha256.create();
  let size = 0;
  try {
    requireCondition(response.ok, 'image-http', `${image.partition}: download failed (HTTP ${response.status}).`);
    requireCondition(response.body, 'image-stream', `${image.partition}: the download has no body.`);
    reader = response.body.getReader();
    while (true) {
      const { value, done } = await reader.read();
      if (done) break;
      size += value.byteLength;
      requireCondition(size <= image.size, 'image-size', `${image.partition}: download is larger than the release metadata.`);
      hash.update(value);
      await sink.write(value);
      onProgress(size, image.size);
    }
    requireCondition(size === image.size, 'image-size', `${image.partition}: incomplete download.`);
    const digest = Array.from(hash.digest(), byte => byte.toString(16).padStart(2, '0')).join('');
    requireCondition(digest === image.sha256, 'image-checksum', `${image.partition}: SHA-256 verification failed.`);
    await sink.close();
    return { size, sha256: digest };
  } catch (error) {
    await reader?.cancel().catch(() => {});
    await sink.abort().catch(() => {});
    throw error;
  } finally {
    hash.destroy();
    reader?.releaseLock();
  }
}

export async function requireStorage(storage, total) {
  requireCondition(storage?.getDirectory && storage?.estimate, 'storage-unavailable', 'This browser does not provide the local storage required to stage a release.');
  const { quota, usage } = await storage.estimate();
  requireCondition(Number.isFinite(quota) && Number.isFinite(usage) && quota - usage >= total + 64 * 1024 ** 2,
    'storage-full', 'There is not enough browser storage for the complete release. Free disk space and retry.');
}

export async function createStagingDirectory(storage, id) {
  const root = await storage.getDirectory();
  const parent = await root.getDirectoryHandle('octosense-installer', { create: true });
  const directory = await parent.getDirectoryHandle(id, { create: true });
  return { directory, dispose: () => parent.removeEntry(id, { recursive: true }) };
}

export function downloadError(error) {
  return { code: error.code ?? 'download-failed', message: error instanceof InstallerError ? error.message : 'The release could not be staged. Check the connection and available disk space.' };
}
