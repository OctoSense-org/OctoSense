export const RECIPE = 'enchilada-fastboot-v1';
export const PARTITIONS = Object.freeze(['system', 'vendor', 'dtbo', 'vbmeta', 'boot']);
export const MAX_IMAGE_BYTES = 16 * 1024 ** 3;
export const MAX_RAW_BYTES = 64 * 1024 ** 2;
export const TRANSFER_CAP = 50 * 1024 ** 2;

export class InstallerError extends Error {
  constructor(code, message) {
    super(message);
    this.name = 'InstallerError';
    this.code = code;
  }
}

export function requireCondition(condition, code, message) {
  if (!condition) throw new InstallerError(code, message);
}

const object = value => value !== null && typeof value === 'object' && !Array.isArray(value);
const text = value => typeof value === 'string' && value.length > 0 && value.length <= 160 && !/[\x00-\x1f]/.test(value);
const positiveSize = value => Number.isSafeInteger(value) && value > 0 && value <= MAX_IMAGE_BYTES;

/** Parse untrusted JSON into an immutable, allowlisted development release. */
export function validateManifest(input, manifestUrl) {
  const invalid = message => requireCondition(false, 'manifest-invalid', message);
  if (!object(input) || input.schema !== 1) invalid('Regenerate this release with manifest schema 1.');
  const keys = ['schema', 'name', 'built', 'device', 'recipe', 'channel', 'images', 'incremental'];
  if (Object.keys(input).some(key => !keys.includes(key))) invalid('Release contains unsupported metadata.');
  if (!text(input.name) || !text(input.built) || !/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$/.test(input.built) || !Number.isFinite(Date.parse(input.built))) invalid('Release name or build date is invalid.');
  if (input.device !== 'enchilada' || input.recipe !== RECIPE) invalid('This preview supports the OnePlus 6 recipe only.');
  if (input.channel !== 'development') invalid('Public release signature verification is not configured in this preview.');
  if (input.incremental !== undefined && !text(input.incremental)) invalid('Invalid build identity.');
  const base = new URL(manifestUrl);
  if (!['http:', 'https:'].includes(base.protocol) || base.username || base.password) invalid('Invalid release location.');
  if (!Array.isArray(input.images) || input.images.length !== PARTITIONS.length) invalid('Release must contain all five required images.');
  const images = new Map();
  for (const image of input.images) {
    if (!object(image) || Object.keys(image).some(key => !['partition', 'file', 'size', 'expanded_size', 'sparse', 'sha256', 'slot'].includes(key))) invalid('Invalid image metadata.');
    if (!PARTITIONS.includes(image.partition) || images.has(image.partition)) invalid('Unexpected or duplicate partition.');
    if (image.file !== `${image.partition}.img`) invalid('Image filenames must match the supported partition.');
    if (!positiveSize(image.size) || !positiveSize(image.expanded_size)) invalid('Invalid image size.');
    if (typeof image.sparse !== 'boolean' || (!image.sparse && (image.expanded_size !== image.size || image.size > MAX_RAW_BYTES))) invalid('Large raw images must be converted to sparse images before packaging.');
    if (typeof image.sha256 !== 'string' || !/^[a-f0-9]{64}$/.test(image.sha256)) invalid('Invalid SHA-256 digest.');
    if (image.slot !== (image.partition !== 'vbmeta')) invalid('Partition slot policy does not match the device recipe.');
    const url = new URL(image.file, base);
    url.searchParams.set('sha256', image.sha256);
    images.set(image.partition, Object.freeze({ ...image, url: url.href }));
  }
  const ordered = PARTITIONS.map(partition => images.get(partition));
  const size = ordered.reduce((sum, image) => sum + image.size, 0);
  if (size > 32 * 1024 ** 3) invalid('Release exceeds the preview storage limit.');
  return Object.freeze({ schema: 1, name: input.name, built: input.built, device: input.device,
    recipe: input.recipe, channel: input.channel, incremental: input.incremental ?? null,
    images: Object.freeze(ordered), size });
}

export async function loadManifest(url, fetcher = fetch) {
  const response = await fetcher(url, { cache: 'no-store', credentials: 'omit', redirect: 'error', signal: AbortSignal.timeout(30_000) });
  requireCondition(response.ok, 'manifest-http', `Release metadata could not be downloaded (HTTP ${response.status}).`);
  const raw = await response.text();
  requireCondition(raw.length <= 64 * 1024, 'manifest-invalid', 'Release metadata is too large.');
  let input;
  try { input = JSON.parse(raw); } catch { throw new InstallerError('manifest-invalid', 'Release metadata is not valid JSON.'); }
  return validateManifest(input, url);
}

export function localWritePolicy(location) {
  return ['http:', 'https:'].includes(location.protocol) &&
    ['localhost', '127.0.0.1', '[::1]'].includes(location.hostname);
}

export function assertIdentity(target) {
  requireCondition(target && text(target.serial) && target.serial !== '?', 'identity-unknown', 'A stable device identity could not be read.');
  const exact = (target.product === 'enchilada' && (!target.codename || target.codename === 'enchilada')) ||
    (target.product === 'sdm845' && target.codename === 'enchilada');
  requireCondition(exact, 'model-unverified', 'The phone model is not verified. The shared sdm845 identifier is insufficient; this connection is read-only.');
  requireCondition(target.userspace === 'no', 'mode-unverified', 'Connect in the device bootloader; its mode must be confirmed.');
  requireCondition(['a', 'b'].includes(target.slot), 'slot-unknown', 'The current boot slot could not be verified.');
  requireCondition(['yes', 'no'].includes(target.unlocked), 'unlock-unknown', 'The bootloader unlock state could not be verified.');
}

export function assertTarget(target, manifest, expected = null, unlocked = true) {
  assertIdentity(target);
  if (unlocked) requireCondition(target.unlocked === 'yes', 'bootloader-locked', 'Unlock the bootloader before preparing an installation.');
  if (expected) requireCondition(['serial', 'product', 'codename', 'slot', 'unlocked'].every(key => target[key] === expected[key]),
    'device-changed', 'The device or boot slot changed. Reconnect and review a new installation.');
  requireCondition(Number.isSafeInteger(target.maxDownload) && target.maxDownload >= 1024 * 1024, 'transfer-limit-unknown', 'The device transfer limit could not be verified.');
  for (const image of manifest.images) {
    const partition = target.partitions?.[image.partition];
    requireCondition(partition && Number.isSafeInteger(partition.size) && partition.size >= image.expanded_size,
      'partition-mismatch', `${image.partition}: the partition size is unknown or smaller than the expanded image.`);
    requireCondition(partition.slotted === image.slot, 'partition-mismatch', `${image.partition}: unexpected slot layout.`);
    if (!image.sparse || !image.slot) requireCondition(image.size <= Math.min(target.maxDownload, TRANSFER_CAP),
      'transfer-limit', `${image.partition}: this image cannot be transferred within the device limit. Repackage it before installation.`);
  }
  return structuredClone(target);
}
