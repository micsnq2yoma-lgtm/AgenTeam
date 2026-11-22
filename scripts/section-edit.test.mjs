import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdtempSync, readFileSync, readdirSync, rmSync, utimesSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import test from 'node:test';

const root = resolve(import.meta.dirname, '..');
const SCRIPT = join(root, 'bundled-skills/agenteam-memory-gardener/scripts/section_edit.py');
const python = process.platform === 'win32' ? 'python' : 'python3';
const sha = text => createHash('sha256').update(text).digest('hex');

function workspace(t, content) {
  const dir = mkdtempSync(join(tmpdir(), 'section-edit-'));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const file = join(dir, 'USER.md');
  writeFileSync(file, content);
  return { dir, file };
}
function edit({ dir, file }, edits, ...args) {
  const edits_json = join(dir, 'edits.json');
  writeFileSync(edits_json, JSON.stringify(edits));
  const result = spawnSync(python, ['-B', SCRIPT, '--file', file, '--edits', edits_json, ...args], { encoding: 'utf8' });
  return { status: result.status, out: result.stdout ? JSON.parse(result.stdout) : null, stderr: result.stderr };
}
const show = ({ file }) => JSON.parse(spawnSync(python, ['-B', SCRIPT, '--file', file, '--show'], { encoding: 'utf8' }).stdout);

const DOC = `# USER.md

preface line

## 基本信息

- 名字：A

## 背景

<!-- 写：现在做什么。由：画像 Task。 -->

旧背景

## 偏好

- 简短

## 近期观察

- (2026-11-01) 喜欢表格
`;

test('--show reports the file hash and the section titles', t => {
  const w = workspace(t, DOC);
  const info = show(w);
  assert.equal(info.sha256, sha(DOC));
  assert.deepEqual(info.sections, ['基本信息', '背景', '偏好', '近期观察']);
});

test('replace, append, add and delete change only the named sections', t => {
  const w = workspace(t, DOC);
  const { status, out } = edit(w, [
    { op: 'replace', heading: '背景', content: '新背景' },
    { op: 'append', heading: '近期观察', content: '- (2026-11-22) 周报用表格' },
    { op: 'delete', heading: '偏好' },
    { op: 'add', heading: '来源概况', content: '- 简历' },
  ]);
  assert.equal(status, 0);
  assert.deepEqual(out.changed, ['背景', '近期观察', '偏好', '来源概况']);
  const after = readFileSync(w.file, 'utf8');
  // Untouched parts are identical, and replace keeps the chapter's ownership hint.
  assert.ok(after.startsWith('# USER.md\n\npreface line\n\n## 基本信息\n\n- 名字：A\n\n## 背景\n'));
  assert.match(after, /<!-- 写：现在做什么。由：画像 Task。 -->\n\n新背景/);
  assert.doesNotMatch(after, /旧背景|## 偏好/);
  assert.match(after, /- \(2026-11-01\) 喜欢表格\n- \(2026-11-22\) 周报用表格\n/);
  assert.match(after, /## 来源概况\n\n- 简历\n$/);
  assert.equal(out.sha256, sha(after));
});

test('a stale base hash is refused, so a concurrent edit is never overwritten', t => {
  const w = workspace(t, DOC);
  const base = show(w).sha256;
  // First writer wins.
  assert.equal(edit(w, [{ op: 'append', heading: '近期观察', content: '- first' }], '--base-sha', base).status, 0);
  const afterFirst = readFileSync(w.file, 'utf8');
  // Second writer started from the same read and must not clobber it.
  const second = edit(w, [{ op: 'replace', heading: '背景', content: 'second' }], '--base-sha', base);
  assert.equal(second.status, 2);
  assert.equal(second.out.code, 'FILE_CHANGED');
  assert.equal(readFileSync(w.file, 'utf8'), afterFirst);
  // After reading again it can retry.
  assert.equal(edit(w, [{ op: 'replace', heading: '背景', content: 'second' }], '--base-sha', show(w).sha256).status, 0);
});

test('--allow enforces chapter ownership and applies nothing when one edit is refused', t => {
  const w = workspace(t, DOC);
  const result = edit(w, [
    { op: 'append', heading: '近期观察', content: '- ok' },
    { op: 'replace', heading: '基本信息', content: '- 名字：B' },
  ], '--allow', '近期观察,背景');
  assert.equal(result.status, 3);
  assert.equal(result.out.code, 'SECTION_NOT_ALLOWED');
  assert.equal(readFileSync(w.file, 'utf8'), DOC);
});

test('invalid edits are rejected without touching the file', t => {
  for (const [edits, code] of [
    [[{ op: 'replace', heading: '背景', content: '## 偷偷加章节\nx' }], 'INVALID_CONTENT'],
    [[{ op: 'replace', heading: '不存在', content: 'x' }], 'SECTION_MISSING'],
    [[{ op: 'add', heading: '背景', content: 'x' }], 'SECTION_EXISTS'],
    [[{ op: 'delete', heading: '背景', content: 'x' }], 'INVALID_EDIT'],
    [[{ op: 'rewrite-everything', heading: '背景', content: 'x' }], 'INVALID_EDIT'],
    [[], 'INVALID_EDIT'],
  ]) {
    const w = workspace(t, DOC);
    const result = edit(w, edits);
    assert.equal(result.status, 5, code);
    assert.equal(result.out.code, code);
    assert.equal(readFileSync(w.file, 'utf8'), DOC);
  }
});

test('a "## " line inside fenced code is content, in the file and in an edit', t => {
  const doc = '## 偏好\n\n```\n## not a heading\n```\n\n## 近期观察\n\n- a\n';
  const w = workspace(t, doc);
  assert.deepEqual(show(w).sections, ['偏好', '近期观察']);
  const result = edit(w, [{ op: 'append', heading: '偏好', content: '```\n## still content\n```' }]);
  assert.equal(result.status, 0);
  assert.match(readFileSync(w.file, 'utf8'), /## still content/);
});

test('a duplicated title is ambiguous and is never guessed', t => {
  const w = workspace(t, '## 偏好\n\na\n\n## 偏好\n\nb\n');
  const result = edit(w, [{ op: 'replace', heading: '偏好', content: 'c' }]);
  assert.equal(result.status, 5);
  assert.equal(result.out.code, 'AMBIGUOUS_HEADING');
});

test('--max-bytes refuses a result over the limit', t => {
  const w = workspace(t, DOC);
  const result = edit(w, [{ op: 'append', heading: '近期观察', content: 'x'.repeat(500) }], '--max-bytes', String(Buffer.byteLength(DOC) + 100));
  assert.equal(result.status, 4);
  assert.equal(result.out.code, 'TOO_LARGE');
  assert.equal(readFileSync(w.file, 'utf8'), DOC);
});

test('CRLF files keep their line endings and no temp file is left behind', t => {
  const w = workspace(t, DOC.replace(/\n/g, '\r\n'));
  assert.equal(edit(w, [{ op: 'append', heading: '近期观察', content: '- new' }]).status, 0);
  const after = readFileSync(w.file, 'utf8');
  assert.ok(!/[^\r]\n/.test(after), 'every newline is CRLF');
  assert.deepEqual(readdirSync(w.dir).sort(), ['USER.md', 'edits.json']);
});

test('works on the shipped USER template: chapters keep their ownership hints', t => {
  const template = readFileSync(join(root, 'src/shared/default-user.md'), 'utf8');
  const w = workspace(t, template);
  const result = edit(w, [
    { op: 'replace', heading: '背景', content: '在一家小公司做产品。' },
    { op: 'append', heading: '近期观察', content: '- (2026-11-22) 喜欢简短回复' },
  ], '--allow', '背景,近期观察', '--max-bytes', '20480');
  assert.equal(result.status, 0);
  const after = readFileSync(w.file, 'utf8').replace(/\r\n/g, '\n'); // a Windows checkout turns the template into CRLF
  assert.match(after, /## 背景\n\n<!-- 写：现在做什么[^\n]*由：用户资料整理[^\n]*-->\n\n在一家小公司做产品。\n/);
  assert.deepEqual(show(w).sections, ['基本信息', '背景', '沟通风格', '工作方式', '偏好', '写作风格', '近期观察', '来源概况']);
});

test('the three skills ship the same section_edit.py', () => {
  const source = readFileSync(SCRIPT, 'utf8');
  for (const skill of ['agenteam-memory-update', 'agenteam-user-profile']) {
    assert.equal(readFileSync(join(root, 'bundled-skills', skill, 'scripts/section_edit.py'), 'utf8'), source, skill);
  }
});

test('each skill allows exactly the chapters it owns, and no two flows own the same chapter', () => {
  const read = name => readFileSync(join(root, 'bundled-skills', name, 'SKILL.md'), 'utf8');
  const allowed = name => {
    const match = read(name).match(/--allow ([^\s`]+)/);
    assert.ok(match, `${name} names its allowed chapters`);
    return new Set(match[1].split(','));
  };
  const update = allowed('agenteam-memory-update');
  const gardener = allowed('agenteam-memory-gardener');
  const profile = allowed('agenteam-user-profile');
  assert.deepEqual([...update], ['近期观察']);
  // Neither the gardener nor the profile task may rewrite the user's own facts or each other's log.
  assert.ok(!gardener.has('基本信息') && !gardener.has('来源概况'));
  assert.ok(!profile.has('基本信息') && !profile.has('近期观察'));
  // The chapters the profile task writes are the ones the gardener merges into, plus the source summary.
  for (const chapter of ['背景', '沟通风格', '工作方式', '偏好', '写作风格']) {
    assert.ok(gardener.has(chapter) && profile.has(chapter), chapter);
  }
  // Only the profile task writes the source summary; only update and gardener touch the observation log.
  assert.ok(profile.has('来源概况') && !update.has('来源概况'));
  assert.ok(gardener.has('近期观察') && update.has('近期观察'));
});

test('two writers from the same version cannot both succeed (no lost update)', async t => {
  const w = workspace(t, DOC);
  const base = sha(DOC);
  const start = marker => {
    const edits = join(w.dir, `edits-${marker}.json`);
    writeFileSync(edits, JSON.stringify([{ op: 'append', heading: '近期观察', content: `- (2026-11-22) ${marker}` }]));
    return new Promise(done => {
      // Widen the window between reading the file and writing it, so a missing lock shows up every time.
      const harness = [
        'import sys, time, runpy, pathlib',
        'real = pathlib.Path.read_bytes',
        'def slow(self):',
        '    data = real(self)',
        "    if self.name == 'USER.md': time.sleep(0.4)",
        '    return data',
        'pathlib.Path.read_bytes = slow',
        'sys.argv = sys.argv[1:]',
        'runpy.run_path(sys.argv[0], run_name="__main__")',
      ].join('\n');
      const child = spawn(python, ['-B', '-c', harness, SCRIPT, '--file', w.file, '--edits', edits, '--base-sha', base], { stdio: 'ignore' });
      child.on('close', code => done(code));
    });
  };
  for (let round = 0; round < 2; round++) {
    writeFileSync(w.file, DOC);
    const codes = await Promise.all([start('alpha'), start('beta')]);
    assert.deepEqual([...codes].sort(), [0, 2], `round ${round}: ${codes}`);
    const text = readFileSync(w.file, 'utf8');
    assert.equal(['alpha', 'beta'].filter(name => text.includes(name)).length, 1);
    assert.equal(readdirSync(w.dir).filter(name => name.endsWith('.lock')).length, 0);
  }
});

test('a lock left by a crashed writer expires instead of blocking forever', t => {
  const w = workspace(t, DOC);
  const lock = join(w.dir, '.USER.md.lock');
  writeFileSync(lock, '');
  const old = new Date(Date.now() - 120_000);
  utimesSync(lock, old, old);
  const result = edit(w, [{ op: 'append', heading: '偏好', content: '- x' }]);
  assert.equal(result.status, 0);
});
