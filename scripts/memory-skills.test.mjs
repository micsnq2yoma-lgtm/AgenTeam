import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import test from 'node:test';
process.env.PYTHONUTF8 = '1';
process.env.PYTHONIOENCODING = 'utf-8';

const root = resolve(import.meta.dirname, '..');
const cases = JSON.parse(readFileSync(join(root, 'src/shared/__fixtures__/memory-rule-discovery.json'), 'utf8'));
const scripts = {
  gardener: join(root, 'bundled-skills/agenteam-memory-gardener/scripts/memory_lint.py'),
  molt: join(root, 'bundled-skills/agenteam-memory-molt/scripts/prepare_molt.py'),
};
function workspace(t) {
  const repo = mkdtempSync(join(tmpdir(), 'memory-architecture-'));
  t.after(() => rmSync(repo, { recursive: true, force: true }));
  mkdirSync(join(repo, '.claude/rules'), { recursive: true });
  return repo;
}
function run(kind, repo, ...args) {
  const result = spawnSync(process.platform === 'win32' ? 'python' : 'python3',
    ['-B', scripts[kind], '--repo', repo, ...args], { encoding: 'utf8', cwd: repo });
  assert.equal(result.status, 0, result.error?.message || result.stderr);
  return result.stdout;
}
function rule(repo, name, content = '- Existing principle\n') {
  writeFileSync(join(repo, '.claude/rules', name), content);
}
function progress(repo) {
  const folder = join(repo, 'memory/molts');
  return readFileSync(join(folder, readdirSync(folder).find(name => name.endsWith('.progress.md'))), 'utf8');
}
for (const { name, matches } of cases) {
  test(`Rust and both maintenance scripts recognize the same memory filename: ${name}`, t => {
    const repo = workspace(t);
    rule(repo, name, '- unique belief\n');
    const report = JSON.parse(run('gardener', repo, '--json'));
    assert.equal(report.some(item => item.check === 'budget' && item.message.includes(name)), matches);
    assert.ok(!report.some(item => item.check === 'substrate' && item.level !== 'INFO'));
    run('molt', repo, '--init-progress');
    assert.equal(progress(repo).includes(`${name}: unique belief`), matches);
    assert.deepEqual(readdirSync(join(repo, '.claude/rules')), [name]);
  });
}
test('custom and multiple memory files are all audited without synthesizing optional layers', t => {
  const repo = workspace(t);
  rule(repo, '06-MEMORY.md', '- first principle\n');
  rule(repo, 'other-MeMoRy.MD', '- second principle; memory/topics/missing.md\n' + 'x'.repeat(52 * 1024));
  rule(repo, 'custom-rules.md');
  const report = JSON.parse(run('gardener', repo, '--json'));
  assert.ok(report.some(item => item.check === 'pointer' && item.level === 'RED'));
  assert.ok(report.some(item => item.check === 'budget' && item.level === 'RED' && item.message.includes('other-MeMoRy.MD')));
  const preparation = run('molt', repo);
  for (const name of ['06-MEMORY.md', 'other-MeMoRy.MD', 'custom-rules.md']) assert.ok(preparation.includes(name));
  assert.ok(!preparation.includes('MISSING'));
  run('molt', repo, '--init-progress');
  assert.ok(progress(repo).includes('first principle'));
  assert.ok(progress(repo).includes('second principle'));
  assert.equal(readdirSync(join(repo, '.claude/rules')).length, 3);
});
test('empty or absent rule directory does not demand a default file set', t => {
  const repo = workspace(t);
  for (const remove of [false, true]) {
    if (remove) rmSync(join(repo, '.claude'), { recursive: true });
    const report = JSON.parse(run('gardener', repo, '--json'));
    assert.ok(!report.some(item => item.check === 'substrate' && item.level !== 'INFO'));
    assert.ok(!run('molt', repo).includes('MISSING'));
  }
});
test('nested files, directories and symlinks do not masquerade as direct rules', { skip: process.platform === 'win32' }, t => {
  const repo = workspace(t);
  const rules = join(repo, '.claude/rules');
  mkdirSync(join(rules, 'nested-memory.md'));
  writeFileSync(join(rules, 'nested-memory.md/MEMORY.md'), '- nested\n');
  writeFileSync(join(repo, 'external.md'), '- linked\n');
  symlinkSync(join(repo, 'external.md'), join(rules, 'link-memory.md'));
  const report = JSON.parse(run('gardener', repo, '--json'));
  assert.ok(!report.some(item => item.check === 'budget'));
  run('molt', repo, '--init-progress');
  assert.ok(!progress(repo).includes(': linked'));
  assert.ok(!progress(repo).includes(': nested'));
});

function observations(repo) {
  return JSON.parse(run('gardener', repo, '--json')).filter(item => item.check === 'observations');
}
const entries = count => Array.from({ length: count }, (_, index) => `- (2026-11-${String(index % 28 + 1).padStart(2, '0')}) observation ${index}`).join('\n');
const userFile = body => `# USER.md\n\n## 基本信息\n\n- 名字：A\n\n## 近期观察\n\n${body}\n\n## 来源概况\n\n- 简历\n`;

test('the shipped USER template reports an empty 近期观察 chapter without a warning', t => {
  const repo = workspace(t);
  rule(repo, '03-USER.md', readFileSync(join(root, 'src/shared/default-user.md'), 'utf8'));
  const [item, ...rest] = observations(repo);
  assert.equal(rest.length, 0);
  assert.equal(item.level, 'INFO');
  assert.match(item.message, /近期观察 0 条/);
});

test('近期观察 over twenty entries asks the Gardener to merge them into the chapters', t => {
  const repo = workspace(t);
  rule(repo, '03-USER.md', userFile(entries(20)));
  assert.equal(observations(repo)[0].level, 'INFO');
  rule(repo, '03-USER.md', userFile(entries(21)));
  const [warning] = observations(repo);
  assert.equal(warning.level, 'WARN');
  assert.match(warning.message, /近期观察 21 条/);
  assert.match(warning.message, /并入对应章节/);
});

test('an older free-form USER file is never judged against the chapter structure', t => {
  const repo = workspace(t);
  rule(repo, '03-USER.md', `# USER.md\n\n## Context\n\n${entries(40)}\n`);
  assert.deepEqual(observations(repo), []);
});

test('only bullets of the 近期观察 chapter count, and fenced code is ignored', t => {
  const repo = workspace(t);
  const other = `## 偏好\n\n${entries(30)}\n\n`;
  const fenced = '```\n' + entries(30) + '\n```';
  rule(repo, '03-USER.md', `# USER.md\n\n${other}## 近期观察\n\n${entries(3)}\n\n${fenced}\n`);
  assert.match(observations(repo)[0].message, /近期观察 3 条/);
});

test('the memory skills describe the same chapter ownership as the profile skill', () => {
  const read = name => readFileSync(join(root, 'bundled-skills', name, 'SKILL.md'), 'utf8').replace(/\r\n/g, '\n');
  const update = read('agenteam-memory-update');
  const gardener = read('agenteam-memory-gardener');
  const profile = read('agenteam-user-profile');
  assert.match(update, /只往"近期观察"追加/);
  assert.match(update, /背景、沟通风格、工作方式、偏好、写作风格、来源概况由画像 Task/);
  assert.match(update, /没有"近期观察"章节的旧式 USER 文件，沿用原有做法/);
  assert.match(gardener, /唯一整理主体章节的入口/);
  assert.match(gardener, /并入后\*\*删除\*\*该条/);
  assert.match(gardener, /没有"近期观察"章节的旧式 USER 文件，沿用原有整编方式/);
  // The profile skill leaves exactly the chapters the other two skills own.
  assert.match(profile, /\*\*近期观察\*\*[^\n]*不是你负责的章节/);
});
