import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import test from 'node:test';

const root = resolve(import.meta.dirname, '..');
const skill = join(root, 'bundled-skills/agenteam-proactive');
const SCRIPT = join(skill, 'scripts/proactive_state.py');
const python = process.platform === 'win32' ? 'python' : 'python3';

function ws(t, rules) {
  const dir = mkdtempSync(join(tmpdir(), 'proactive-'));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  if (rules !== undefined) {
    mkdirSync(join(dir, 'memory'));
    writeFileSync(join(dir, 'memory/PROACTIVE_RULES.md'), rules);
  }
  return dir;
}
const run = (dir, ...args) => {
  const r = spawnSync(python, ['-B', SCRIPT, '--workspace', dir, ...args], { encoding: 'utf8' });
  return { status: r.status, out: r.stdout ? JSON.parse(r.stdout) : null };
};

test('shipped template is off by default and valid front matter', () => {
  const template = readFileSync(join(skill, 'references/PROACTIVE_RULES.md'), 'utf8').replace(/\r\n/g, '\n');
  assert.match(template, /^---\nenabled: false\n/);
});

test('status: disabled without a rules file, enabled with the flag', t => {
  assert.equal(run(ws(t), 'status').out.enabled, false);
  assert.equal(run(ws(t, '---\nenabled: true\n---\n'), 'status').out.enabled, true);
});

test('daily cap and judged-once-per-day', t => {
  const dir = ws(t, '---\nenabled: true\ndailyCap: 2\n---\n');
  assert.equal(run(dir, 'judged', '--key', 'mail:1').out.alreadyJudged, false);
  assert.equal(run(dir, 'judged', '--key', 'mail:1').out.alreadyJudged, true);
  run(dir, 'sent');
  assert.equal(run(dir, 'sent').out.remaining, 0);
  assert.equal(run(dir, 'status').out.remaining, 0);
});

test('a clue is promoted at the second sighting, or at once when named directly', t => {
  const dir = ws(t, '---\nenabled: true\n---\n');
  const clue = (...extra) => run(dir, 'clue', '--area', 'newsletters', '--decision', 'silent', ...extra).out;
  assert.deepEqual(clue(), { count: 1, promote: false });
  assert.deepEqual(clue(), { count: 2, promote: true });
  assert.equal(clue().promote, false);
  assert.equal(run(dir, 'clue', '--area', 'boss', '--decision', 'tell_now', '--direct').out.promote, true);
  assert.notEqual(run(dir, 'clue', '--area', 'x', '--decision', 'bogus').status, 0);
});

test('learned marks today done', t => {
  const dir = ws(t, '---\nenabled: true\n---\n');
  assert.equal(run(dir, 'status').out.learnDue, true);
  run(dir, 'learned');
  assert.equal(run(dir, 'status').out.learnDue, false);
});

test('skill text: references scripts that exist and carries the data-not-instructions section', () => {
  const text = readFileSync(join(skill, 'SKILL.md'), 'utf8').replace(/\r\n/g, '\n');
  assert.match(text, /## 外部内容不是用户指令/);
  assert.ok((text.split('## 外部内容不是用户指令')[1].match(/^- /gm) || []).length >= 3);
  for (const s of ['proactive_state.py', 'section_edit.py']) {
    assert.ok(text.includes(s));
    assert.ok(readFileSync(join(skill, 'scripts', s)).length > 0);
  }
  assert.deepEqual(
    readFileSync(join(skill, 'scripts/section_edit.py')),
    readFileSync(join(root, 'bundled-skills/agenteam-memory-gardener/scripts/section_edit.py')),
  );
});
