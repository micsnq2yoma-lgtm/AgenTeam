const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const { createReadStream } = require('node:fs');
const { createHash } = require('node:crypto');
const path = require('node:path');
const { spawn } = require('node:child_process');
const { setTimeout: wait } = require('node:timers/promises');

const installDir = path.join(process.env.LOCALAPPDATA, 'AgenTeam');
const exe = path.join(installDir, 'AgenTeam.exe');
const profile = path.join(process.env.USERPROFILE, '.agenteam');
const evidence = path.join(process.env.QA_ROOT, 'evidence');
const { chromium } = require(path.join(installDir, 'node_modules/playwright-core'));

async function digest(file) {
  const hash = createHash('sha256');
  for await (const chunk of createReadStream(file)) hash.update(chunk);
  return hash.digest('hex');
}

async function retry(description, action, timeout) {
  const end = Date.now() + timeout;
  let lastError;
  while (Date.now() < end) {
    try {
      const result = await action();
      if (result) return result;
    } catch (error) {
      lastError = error;
    }
    await wait(750);
  }
  throw new Error(description, { cause: lastError });
}

async function connect(version) {
  const browser = await retry('WebView2 did not start', () =>
    chromium.connectOverCDP('http://127.0.0.1:19222', { timeout: 2500 }), 90000);
  const page = await retry('AgenTeam window not available for ' + version, async () => {
    for (const context of browser.contexts()) {
      for (const window of context.pages()) {
        const isMain = await window.evaluate(async (expected) => {
          const tauri = globalThis.__TAURI_INTERNALS__;
          return tauri?.metadata?.currentWindow?.label === 'main' &&
            await tauri.invoke('plugin:app|version') === expected;
        }, version).catch(() => false);
        if (isMain) return window;
      }
    }
  }, 60000);
  await page.setViewportSize({ width: 1360, height: 800 });
  return page;
}

async function main() {
  assert.equal(process.env.GITHUB_ACTIONS, 'true');
  assert.equal(process.env.RUNNER_OS, 'Windows');
  assert.equal(process.env.GITHUB_REPOSITORY, 'micsnq2yoma-lgtm/AgenTeam');
  assert.equal(await digest(exe), 'fd5e65f252fd93a6f5dff829cf9ade81faa9eaac40508c1c9fccbc3302030d8b');
  await fs.mkdir(profile, { recursive: true });
  await fs.mkdir(evidence, { recursive: true });
  const config = path.join(profile, 'config.json');
  await fs.writeFile(config, JSON.stringify({ uiLanguage: 'en-US', appearanceMode: 'dark' }), { flag: 'wx' });
  const app = spawn(exe, [], { cwd: installDir, windowsHide: true, stdio: 'ignore' });
  app.unref();
  const page = await connect('0.6.1');
  await page.getByRole('button', { name: 'Settings', exact: true }).first().click({ timeout: 15000 });
  await page.getByRole('button', { name: 'About', exact: true }).click({ timeout: 15000 });
  await page.getByRole('button', { name: 'Check for Updates', exact: true }).click({ timeout: 15000 });
  if (process.env.QA_LATEST === '0.6.1') {
    await page.getByText('You are already on the latest version', { exact: true }).waitFor({ timeout: 25000 });
    await page.screenshot({ path: path.join(evidence, 'manual-up-to-date.png') });
    console.log('0.6.1 manual Check for Updates reported latest on the public channel.');
    return;
  }
  assert.equal(process.env.QA_LATEST, '0.6.2');
  const restart = page.getByRole('button', { name: /^restart update$/i }).first();
  await restart.waitFor({ timeout: 240000 });
  const pending = path.join(profile, 'updates/pending_update.bin');
  const expected = process.env.QA_PACKAGE_DIGEST;
  assert.match(expected, /^[a-f0-9]{64}$/);
  assert.equal(await digest(pending), expected);
  await page.screenshot({ path: path.join(evidence, 'manual-update-ready.png') });
  await restart.click({ timeout: 15000 });
  const confirm = page.getByRole('button', { name: /^(exit|quit)$/i }).last();
  if (await confirm.waitFor({ timeout: 5000 }).then(() => true, () => false)) {
    await confirm.click({ timeout: 5000 }).catch(() => {});
  }
  await retry('Original 0.6.1 did not hand off to the installer', () => app.exitCode !== null, 110000);
  await retry('Installed binary was not replaced', async () =>
    await digest(exe) !== 'fd5e65f252fd93a6f5dff829cf9ade81faa9eaac40508c1c9fccbc3302030d8b', 180000);
  const latest = await connect('0.6.2');
  await latest.screenshot({ path: path.join(evidence, 'updated-0.6.2.png') });
  const logDir = path.join(profile, 'logs');
  const logNames = (await fs.readdir(logDir)).filter(name => /^unified-.*\.log$/.test(name));
  const unified = (await Promise.all(logNames.map(name => fs.readFile(path.join(logDir, name), 'utf8')))).join('\n');
  assert.match(unified, /Manual update check requested/);
  await fs.writeFile(path.join(evidence, 'result.json'), JSON.stringify({
    from: '0.6.1', to: '0.6.2', autoRelaunch: true, manualCheck: true, packageSha256: expected
  }, null, 2));
  console.log('The original 0.6.1 manual Check for Updates installed signed public 0.6.2 and relaunched.');
}

main().then(() => process.exit(0), error => { console.error(error); process.exit(1); });
