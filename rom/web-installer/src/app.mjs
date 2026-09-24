import { loadManifest, localWritePolicy } from './contracts.mjs';
import { InstallSession, browserJournal, browserExclusive } from './session.mjs';
import { FastbootTransport } from './transport.mjs';
import { stageRelease } from './staging.mjs';

const $ = id => document.getElementById(id);
const supported = Boolean(isSecureContext && navigator.usb && navigator.locks && navigator.storage?.getDirectory && crypto.subtle);
const allowWrites = supported && localWritePolicy(location);
const logs = [];
let previousPhase = '';
let previousVerification = '';
const labels = {
  disconnected: 'Connect your phone to begin.', connecting: 'Reading the phone’s identity and bootloader state…',
  connected: 'Phone connected. Review compatibility before continuing.', preparing: 'Downloading and verifying the complete release…',
  review: 'All images verified. Review the data-erasure confirmation to continue.', flashing: 'Installing. Keep the phone connected and this tab open.',
  'images-written': 'Images written and data erased. Ready to restart; Android boot has not been verified.',
  'awaiting-boot': 'Restart requested. Wait for OctoSense Home to appear on the phone.',
  'boot-confirmed-by-user': 'You confirmed that OctoSense Home is visible. Automatic build verification is not yet available.',
  'unlock-requested': 'Confirm unlocking on the phone. After it finishes, reconnect to verify the new bootloader state.',
  blocked: 'The operation stopped. Review the message below before continuing.',
  'recovery-required': 'The previous installation needs recovery review before another write.',
};
function log(message) {
  logs.push({ time: new Date().toISOString(), message });
  $('log').textContent = logs.map(line => `${line.time.slice(11, 19)}  ${line.message}`).join('\n');
  $('log').scrollTop = $('log').scrollHeight;
}
function render(state) {
  if (state.phase !== previousPhase) { log(labels[state.phase] ?? state.phase); previousPhase = state.phase; }
  $('stage').textContent = labels[state.phase] ?? state.phase;
  const compatibility = session.compatibility();
  $('compatibility').textContent = state.target ? compatibility ?? 'Device identity and partition layout match this development recipe.' : '';
  $('connect').disabled = !supported || state.busy || ['images-written', 'awaiting-boot'].includes(state.phase);
  $('info').hidden = !state.target;
  if (state.target) {
    $('product').textContent = state.target.product === 'enchilada' || state.target.codename === 'enchilada' ? 'OnePlus 6' : 'Model unverified';
    $('serial').textContent = state.target.serial ? `••••${state.target.serial.slice(-4)}` : 'Unknown';
    $('slot').textContent = state.target.slot ?? 'Unknown';
    $('unlocked').textContent = state.target.unlocked === 'yes' ? 'Unlocked' : state.target.unlocked === 'no' ? 'Locked' : 'Unknown';
  }
  const eligible = !state.busy && !compatibility && !state.recoveryRequired;
  $('prepare').disabled = !eligible || state.target?.unlocked !== 'yes';
  $('wipe').disabled = state.busy || state.phase !== 'review';
  $('install').disabled = !eligible || state.phase !== 'review' || !$('wipe').checked;
  $('unlock-panel').hidden = !state.target || state.target.unlocked !== 'no';
  $('unlock-confirm').disabled = state.busy;
  $('unlock').disabled = !eligible || state.target?.unlocked !== 'no' || !$('unlock-confirm').checked;
  $('reboot').disabled = state.busy || state.phase !== 'images-written';
  $('confirm-boot').hidden = state.phase !== 'awaiting-boot';
  $('confirm-boot').disabled = state.busy;
  $('boot-result').textContent = state.phase === 'boot-confirmed-by-user' ? labels[state.phase] : '';
  $('recovery').hidden = state.phase !== 'recovery-required';
  $('error').hidden = !state.error;
  $('error').textContent = state.error?.message ?? '';
  $('bar').hidden = !state.progress;
  if (state.progress) {
    const progress = state.progress;
    $('bar').value = progress.type === 'flash' ? progress.value : progress.bytes / progress.total || 0;
    if (progress.type === 'progress') $('stage').textContent = `Downloading and verifying ${progress.partition}: ${(progress.bytes / 1048576).toFixed(1)} / ${(progress.total / 1048576).toFixed(1)} MiB`;
    if (progress.type === 'verified' && progress.partition !== previousVerification) { log(`${progress.partition}: full SHA-256 verified`); previousVerification = progress.partition; }
  }
}
const session = new InstallSession({ transport: new FastbootTransport(), prepare: stageRelease,
  journal: browserJournal, exclusive: browserExclusive, allowWrites, onChange: render });

function action(id, operation) {
  $(id).onclick = async () => {
    try { await operation(); }
    catch (error) {
      log(`Stopped: ${error.code ?? 'operation-failed'}`);
      $('error').textContent = error.message;
      $('error').hidden = false;
    }
  };
}
action('connect', () => session.connect());
action('prepare', () => { $('wipe').checked = false; previousVerification = ''; return session.prepare(); });
action('install', () => session.install({ confirmErase: $('wipe').checked }));
action('unlock', () => session.unlock({ confirmErase: $('unlock-confirm').checked }));
action('reboot', () => session.reboot());
action('confirm-boot', () => session.confirmBoot());
for (const id of ['wipe', 'unlock-confirm']) $(id).onchange = () => render(session.snapshot());
$('report').onclick = () => {
  const state = session.snapshot();
  const report = { schema: 1, installer: 'development-preview', release: state.manifest?.name ?? null,
    phase: state.phase, written: state.written, errorCode: state.error?.code ?? null, logs };
  const url = URL.createObjectURL(new Blob([JSON.stringify(report, null, 2)], { type: 'application/json' }));
  const link = document.createElement('a'); link.href = url; link.download = 'octosense-installer-report.json'; link.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
};
window.addEventListener('beforeunload', event => {
  if (session.busy || session.dirty) { event.preventDefault(); event.returnValue = ''; }
});
$('unsupported').hidden = supported;
if (!allowWrites && supported) $('preview').textContent = 'Developer preview. This public page can inspect a phone, but installation is disabled until signed releases and device qualification are ready.';
render(session.snapshot());
try {
  const manifest = await loadManifest(new URL('../manifest.json', import.meta.url).href);
  const add = (key, value) => {
    const dt = document.createElement('dt'); const dd = document.createElement('dd');
    dt.textContent = key; dd.textContent = value; $('release').append(dt, dd);
  };
  add('Release', manifest.name); add('Channel', 'Development'); add('Built', manifest.built);
  add('Phone', 'OnePlus 6 (enchilada)'); add('Download', `${(manifest.size / 1024 ** 3).toFixed(2)} GiB`);
  session.setManifest(manifest);
} catch (error) {
  $('error').textContent = error.message; $('error').hidden = false;
  log(`Release unavailable: ${error.code ?? 'manifest-invalid'}`);
}
