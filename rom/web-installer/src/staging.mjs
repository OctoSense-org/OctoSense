import { InstallerError } from './contracts.mjs';
import { createStagingDirectory } from './download.mjs';

export async function stageRelease(manifest, onProgress) {
  const id = crypto.randomUUID();
  const worker = new Worker(new URL('./download-worker.mjs', import.meta.url), { type: 'module' });
  try {
    await new Promise((resolve, reject) => {
      worker.onerror = () => reject(new InstallerError('worker-failed', 'The verification worker stopped. No partitions were written.'));
      worker.onmessage = ({ data }) => {
        if (data.type === 'ready') resolve();
        else if (data.type === 'error') reject(new InstallerError(data.code, data.message));
        else onProgress(data);
      };
      worker.postMessage({ manifest, id });
    });
    const staging = await createStagingDirectory(navigator.storage, id);
    const files = new Map();
    for (const image of manifest.images) {
      const handle = await staging.directory.getFileHandle(image.file);
      files.set(image.partition, await handle.getFile());
    }
    return { files, dispose: staging.dispose };
  } catch (error) {
    const staging = await createStagingDirectory(navigator.storage, id).catch(() => null);
    await staging?.dispose().catch(() => {});
    throw error;
  } finally {
    worker.terminate();
  }
}
