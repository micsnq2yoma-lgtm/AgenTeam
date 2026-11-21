import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { join } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

if (process.env.GITHUB_ACTIONS !== 'true' || process.env.GITHUB_REPOSITORY !== 'micsnq2yoma-lgtm/AgenTeam' || process.env.RUNNER_OS !== 'Windows') {
  throw new Error('The native recovery test requires a disposable GitHub-hosted Windows runner');
}
const require = createRequire(import.meta.url);
const { chromium } = require(join(process.env.AGENTEAM_QA_INSTALL, 'node_modules', 'playwright-core'));
const deadline = Date.now() + 120000;
let browser;
let last;
try {
  while (Date.now() < deadline) {
    try {
      browser ??= await chromium.connectOverCDP('http://127.0.0.1:19386', { timeout: 2500 });
      for (const context of browser.contexts()) {
        for (const page of context.pages()) {
          const state = await page.evaluate(async () => {
            if (window.__AGENTEAM_RECOVERY_ONLY__ !== true || !window.__TAURI_INTERNALS__) return null;
            const native = await window.__TAURI_INTERNALS__.invoke('cmd_get_egress_restriction');
            return {
              title: document.querySelector('main h1')?.textContent?.trim(),
              field: document.querySelector('main form input')?.value,
              enabled: document.querySelector('main [role=switch]')?.getAttribute('aria-checked'),
              hasStatus: Boolean(document.querySelector('main [role=status]')?.textContent?.trim()),
              nativeState: native.state,
              expectedIp: native.expectedIp,
              observedIp: native.observedIp
            };
          });
          last = state;
          if (!state?.title || !state.hasStatus) continue;
          assert.ok(['AgenTeam is locked', 'AgenTeam 已锁定'].includes(state.title));
          assert.equal(state.field, 'invalid-qa-address');
          assert.equal(state.enabled, 'true');
          assert.equal(state.nativeState, 'unavailable');
          assert.equal(state.expectedIp, 'invalid-qa-address');
          assert.equal(state.observedIp, null);
          console.log('Installed WebView2 recovery window:', JSON.stringify(state));
          process.exitCode = 0;
          await browser.close();
          process.exit(0);
        }
      }
    } catch (err) {
      if (err.code === 'ERR_ASSERTION') throw err;
      last = String(err.message).slice(0, 320);
    }
    await delay(1200);
  }
  throw new Error('Timed out waiting for native recovery UI: ' + JSON.stringify(last));
} finally {
  await browser?.close();
}
