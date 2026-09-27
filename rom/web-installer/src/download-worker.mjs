import { verifyImageStream, requireStorage, createStagingDirectory, downloadError } from './download.mjs';
import { validateImageFormat } from './image-format.mjs';

self.onmessage = async ({ data: { manifest, id } }) => {
  let staging;
  try {
    await requireStorage(navigator.storage, manifest.size);
    staging = await createStagingDirectory(navigator.storage, id);
    for (const image of manifest.images) {
      self.postMessage({ type: 'progress', partition: image.partition, bytes: 0, total: image.size });
      const response = await fetch(image.url, { cache: 'no-store', credentials: 'omit', redirect: 'error', signal: AbortSignal.timeout(15 * 60_000) });
      const handle = await staging.directory.getFileHandle(image.file, { create: true });
      const writer = await handle.createWritable();
      let lastProgress = 0;
      await verifyImageStream(image, response, writer, (bytes, total) => {
        if (performance.now() - lastProgress > 100 || bytes === total) {
          self.postMessage({ type: 'progress', partition: image.partition, bytes, total });
          lastProgress = performance.now();
        }
      });
      await validateImageFormat(image, await handle.getFile());
      self.postMessage({ type: 'verified', partition: image.partition });
    }
    self.postMessage({ type: 'ready' });
  } catch (error) {
    await staging?.dispose().catch(() => {});
    self.postMessage({ type: 'error', ...downloadError(error) });
  }
};
