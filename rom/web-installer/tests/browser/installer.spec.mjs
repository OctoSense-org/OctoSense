import { test, expect } from '@playwright/test';
import { readFile } from 'node:fs/promises';
import { bytes, digest, manifestInput, target } from '../fixtures.mjs';

// Only the USB boundary and release server are simulated. Tests run the real
// page, controller, Web Locks, verification worker and OPFS in Chromium.
const transport = `export class FastbootTransport {
  async connect() { window.calls.push('connect'); }
  async inspect() { return structuredClone(window.phone); }
  async flash(image, file, phone, progress) {
    window.calls.push('flash:' + image.partition);
    if (window.failPartition === image.partition) throw new Error('Simulated USB disconnection');
    if (file.size !== image.size) throw new Error('Invalid staged file');
    progress(1);
  }
  async eraseData() { window.calls.push('erase'); }
  async unlock() { window.calls.push('unlock'); }
  async reboot() { window.calls.push('reboot'); }
}`;
async function setup(context, options = {}) {
  await context.addInitScript(({ phone, unsupported }) => {
    Object.defineProperty(navigator, 'usb', { value: unsupported ? undefined : {}, configurable: true });
    window.phone = phone; window.calls = [];
  }, { phone: options.phone ?? target(), unsupported: options.unsupported ?? false });
  await context.route('**/src/transport.mjs', route => route.fulfill({ contentType: 'text/javascript', body: transport }));
  await context.route('**/manifest.json', route => route.fulfill({ json: options.manifest ?? manifestInput() }));
  await context.route(/\.img\?sha256=/, options.images ?? (route => route.fulfill({ body: Buffer.from(bytes) })));
}
async function connect(page) {
  await page.goto('/');
  await expect(page.locator('#release')).toContainText('Test release');
  await page.locator('#connect').click();
}
async function prepare(page) {
  await connect(page);
  await page.locator('#prepare').click();
  await expect(page.locator('#stage')).toContainText('All images verified');
}
const writes = page => page.evaluate(() => calls.filter(call => call !== 'connect'));
async function stagedDirectories(page) {
  return page.evaluate(async () => {
    const root = await navigator.storage.getDirectory();
    try {
      const staging = await root.getDirectoryHandle('octosense-installer');
      const names = []; for await (const name of staging.keys()) names.push(name); return names;
    } catch { return []; }
  });
}

test('fresh install verifies all files before consent, writes once and checks boot separately', async ({ context, page }) => {
  const errors = []; page.on('pageerror', error => errors.push(error.message));
  await setup(context); await prepare(page);
  expect(await writes(page)).toEqual([]);
  expect(await stagedDirectories(page)).toHaveLength(1);
  await expect(page.locator('#install')).toBeDisabled();
  await page.locator('#wipe').check();
  await page.locator('#install').click();
  await expect(page.locator('#stage')).toContainText('Ready to restart');
  expect(await writes(page)).toEqual(['flash:system', 'flash:vendor', 'flash:dtbo', 'flash:vbmeta', 'flash:boot', 'erase']);
  expect(await stagedDirectories(page)).toEqual([]);
  await page.locator('#reboot').click();
  await expect(page.locator('#stage')).toContainText('Restart requested');
  await page.locator('#confirm-boot').click();
  await expect(page.locator('#boot-result')).toContainText('Automatic build verification is not yet available');
  expect(await page.evaluate(() => localStorage.getItem('octosense-install-journal-v1'))).toBeNull();
  await page.locator('.technical summary').click();
  const downloaded = page.waitForEvent('download');
  await page.locator('#report').click();
  const stream = await (await downloaded).createReadStream();
  const chunks = []; for await (const chunk of stream) chunks.push(chunk);
  const report = Buffer.concat(chunks).toString();
  expect(report).not.toContain(target().serial);
  expect(JSON.parse(report).phase).toBe('boot-confirmed-by-user');
  expect(errors).toEqual([]);
});

test('corruption at the end of a 64 MiB download blocks every write', async ({ context, page }) => {
  const body = Buffer.alloc(64 * 1024 ** 2, 0x5a);
  const manifest = manifestInput();
  Object.assign(manifest.images[0], { size: body.length, expanded_size: body.length, sparse: true, sha256: digest(body) });
  body[body.length - 1] ^= 0xff;
  await setup(context, { manifest, images: route => route.fulfill({ body }) });
  await connect(page); await page.locator('#prepare').click();
  await expect(page.locator('#error')).toContainText('SHA-256', { timeout: 25_000 });
  await expect(page.locator('#install')).toBeDisabled();
  expect(await writes(page)).toEqual([]);
  expect(await stagedDirectories(page)).toEqual([]);
});

test('a missing final download leaves partitions untouched and cleans storage', async ({ context, page }) => {
  await setup(context, { images: route => route.fulfill(route.request().url().includes('/boot.img') ? { status: 404, body: 'missing' } : { body: Buffer.from(bytes) }) });
  await connect(page); await page.locator('#prepare').click();
  await expect(page.locator('#error')).toContainText('404');
  expect(await writes(page)).toEqual([]);
  expect(await stagedDirectories(page)).toEqual([]);
});

test('a hashed image with incorrect sparse metadata stops before the first flash', async ({ context, page }) => {
  const manifest = manifestInput(); manifest.images.at(-1).sparse = true;
  await setup(context, { manifest }); await connect(page); await page.locator('#prepare').click();
  await expect(page.locator('#error')).toContainText('image format');
  expect(await writes(page)).toEqual([]);
  expect(await stagedDirectories(page)).toEqual([]);
});

test('the actual page remains read-only on a public HTTPS origin', async ({ context, page }) => {
  await context.route('https://installer.example.test/**', async route => {
    const path = new URL(route.request().url()).pathname;
    const file = new URL(path === '/' ? '../../index.html' : `../..${path}`, import.meta.url);
    const body = await readFile(file);
    await route.fulfill({ body, contentType: path.endsWith('.mjs') || path.endsWith('.js') ? 'text/javascript' : path.endsWith('.css') ? 'text/css' : 'text/html' });
  });
  await setup(context);
  await page.goto('https://installer.example.test/');
  await expect(page.locator('#release')).toContainText('Test release');
  await page.locator('#connect').click();
  await expect(page.locator('#preview')).toContainText('installation is disabled');
  await expect(page.locator('#prepare')).toBeDisabled();
  await expect(page.locator('#install')).toBeDisabled();
  expect(await writes(page)).toEqual([]);
});

test('invalid release cannot enable install or unlock', async ({ context, page }) => {
  const manifest = manifestInput(); manifest.images = [];
  await setup(context, { manifest, phone: { ...target(), unlocked: 'no' } });
  await page.goto('/'); await expect(page.locator('#error')).toContainText('five required images');
  await page.locator('#connect').click();
  await page.locator('#unlock-confirm').check();
  for (const id of ['prepare', 'install', 'unlock']) await expect(page.locator(`#${id}`)).toBeDisabled();
  expect(await writes(page)).toEqual([]);
});

for (const phone of [{ ...target(), codename: null }, { ...target(), codename: 'fajita', unlocked: 'no' }, { ...target(), unlocked: null }]) {
  test(`unverified phone is read-only: ${phone.codename}/${phone.unlocked}`, async ({ context, page }) => {
    await setup(context, { phone }); await connect(page);
    await expect(page.locator('#prepare')).toBeDisabled();
    await expect(page.locator('#install')).toBeDisabled();
    if (phone.unlocked === 'no') { await page.locator('#unlock-confirm').check(); await expect(page.locator('#unlock')).toBeDisabled(); }
    expect(await writes(page)).toEqual([]);
  });
}

test('download disables conflicting controls and holds the lock across tabs', async ({ context, page }) => {
  let release; const hold = new Promise(resolve => { release = resolve; });
  let started; const downloadStarted = new Promise(resolve => { started = resolve; });
  await setup(context, { images: async route => { started(); await hold; await route.fulfill({ body: Buffer.from(bytes) }); } });
  await connect(page); await page.locator('#prepare').click(); await downloadStarted;
  try {
    for (const id of ['connect', 'prepare', 'install', 'wipe', 'reboot']) await expect(page.locator(`#${id}`)).toBeDisabled();
    const second = await context.newPage(); await second.goto('/'); await second.locator('#connect').click();
    await expect(second.locator('#error')).toContainText('Another OctoSense installer tab');
    expect(await writes(second)).toEqual([]);
  } finally { release(); }
  await expect(page.locator('#stage')).toContainText('All images verified');
});

test('partial write is not retried and recovery journal survives reload', async ({ context, page }) => {
  await setup(context); await prepare(page);
  await page.evaluate(() => { window.failPartition = 'vendor'; });
  await page.locator('#wipe').check(); await page.locator('#install').click();
  await expect(page.locator('#recovery')).toBeVisible();
  expect(await writes(page)).toEqual(['flash:system', 'flash:vendor']);
  await expect(page.locator('#reboot')).toBeDisabled();
  page.on('dialog', dialog => dialog.accept());
  await page.reload();
  await expect(page.locator('#recovery')).toBeVisible();
  await page.locator('#connect').click();
  await expect(page.locator('#prepare')).toBeDisabled();
  await expect(page.locator('#install')).toBeDisabled();
  expect(await writes(page)).toEqual([]);
});

test('unsupported browser explains requirements without enabling connection', async ({ context, page }) => {
  await setup(context, { unsupported: true }); await page.goto('/');
  await expect(page.locator('#unsupported')).toBeVisible();
  await expect(page.locator('#connect')).toBeDisabled();
});

test('narrow screen has no horizontal overflow and consent works with keyboard', async ({ context, page }) => {
  await page.setViewportSize({ width: 360, height: 800 });
  await setup(context); await prepare(page);
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  await page.locator('#wipe').focus(); await page.keyboard.press('Space');
  await expect(page.locator('#install')).toBeEnabled();
});
