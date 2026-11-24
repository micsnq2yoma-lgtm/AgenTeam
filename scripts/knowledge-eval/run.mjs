#!/usr/bin/env node
/**
 * K11 Knowledge Base Retrieval Evaluation Runner.
 *
 * Runs the real Rust `KnowledgeStore` + `KnowledgeIngestManager` + `KnowledgeIndex`
 * (Tantivy BM25 + Jieba tokenizer) in an isolated temp directory against
 * `scripts/knowledge-eval/corpus/` and `scripts/knowledge-eval/qrels.json`,
 * printing Recall@8, MRR@10, nDCG@10, subset breakdowns, and per-query hits.
 */

import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const REPO_ROOT = path.resolve(__dirname, '../..');
const CARGO_MANIFEST = path.join(REPO_ROOT, 'src-tauri', 'Cargo.toml');
const TEST_NAME = 'search::knowledge_indexer::eval::k11_eval_runner_cli';

function parseArgs(argv) {
  const opts = {
    corpus: path.join(__dirname, 'corpus'),
    qrels: path.join(__dirname, 'qrels.json'),
    out: null,
    json: false,
    check: false,
  };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === '--json') {
      opts.json = true;
    } else if (arg === '--check') {
      opts.check = true;
    } else if (arg === '--out' && argv[i + 1]) {
      opts.out = path.resolve(argv[++i]);
    } else if (arg === '--corpus' && argv[i + 1]) {
      opts.corpus = path.resolve(argv[++i]);
    } else if (arg === '--qrels' && argv[i + 1]) {
      opts.qrels = path.resolve(argv[++i]);
    } else if (arg === '--help' || arg === '-h') {
      console.log(
        [
          'Usage: node scripts/knowledge-eval/run.mjs [options]',
          '',
          'Options:',
          '  --corpus <dir>   Corpus directory (default: scripts/knowledge-eval/corpus)',
          '  --qrels <file>   Qrels JSON path (default: scripts/knowledge-eval/qrels.json)',
          '  --out <file>     Write full JSON report to path',
          '  --json           Print raw JSON report to stdout',
          '  --check          Exit non-zero if Recall@8 < 0.70 or MRR@10 < 0.50',
        ].join('\n'),
      );
      process.exit(0);
    } else {
      throw new Error(`Unknown argument: ${arg}`);
    }
  }
  return opts;
}

function buildTestBinary() {
  const res = spawnSync(
    'cargo',
    [
      'test',
      '--manifest-path',
      CARGO_MANIFEST,
      '--lib',
      '--no-run',
      '--message-format=json',
    ],
    {
      cwd: REPO_ROOT,
      encoding: 'utf8',
      maxBuffer: 64 * 1024 * 1024,
    },
  );
  if (res.status !== 0) {
    for (const line of (res.stdout || '').split(/\r?\n/)) {
      if (!line.startsWith('{')) continue;
      try {
        const msg = JSON.parse(line);
        if (msg.reason === 'compiler-message' && msg.message?.rendered) {
          process.stderr.write(msg.message.rendered);
        }
      } catch {
        // ignore
      }
    }
    if (res.stderr) process.stderr.write(res.stderr);
    throw new Error(`cargo test --lib --no-run failed with exit code ${res.status}`);
  }

  let exePath = null;
  for (const line of (res.stdout || '').split(/\r?\n/)) {
    if (!line.startsWith('{')) continue;
    try {
      const msg = JSON.parse(line);
      if (
        msg.reason === 'compiler-artifact' &&
        msg.profile?.test === true &&
        typeof msg.executable === 'string' &&
        msg.executable.length > 0
      ) {
        exePath = msg.executable;
      }
    } catch {
      // ignore non-JSON lines
    }
  }
  if (!exePath || !fs.existsSync(exePath)) {
    throw new Error('Could not locate compiled Rust lib test binary from cargo output');
  }
  return exePath;
}

function findWindowsMtExe() {
  const kitsBin = 'C:\\Program Files (x86)\\Windows Kits\\10\\bin';
  if (fs.existsSync(kitsBin)) {
    const versions = fs
      .readdirSync(kitsBin)
      .filter((name) => /^\d+\./.test(name))
      .sort()
      .reverse();
    for (const ver of versions) {
      const candidate = path.join(kitsBin, ver, 'x64', 'mt.exe');
      if (fs.existsSync(candidate)) return candidate;
    }
  }
  return 'mt.exe';
}

function findTauriWindowsManifest() {
  const meta = spawnSync(
    'cargo',
    ['metadata', '--manifest-path', CARGO_MANIFEST, '--format-version', '1'],
    { cwd: REPO_ROOT, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 },
  );
  if (meta.status !== 0) return null;
  const parsed = JSON.parse(meta.stdout);
  const pkg = parsed.packages?.find((p) => p.name === 'tauri-build');
  if (!pkg?.manifest_path) return null;
  const candidate = path.join(path.dirname(pkg.manifest_path), 'src', 'windows-app-manifest.xml');
  return fs.existsSync(candidate) ? candidate : null;
}

function ensureWindowsManifest(exePath) {
  if (process.platform !== 'win32') return;
  const manifestPath = findTauriWindowsManifest();
  if (!manifestPath) {
    throw new Error('Could not locate tauri-build src/windows-app-manifest.xml');
  }
  const mtExe = findWindowsMtExe();
  const res = spawnSync(
    mtExe,
    ['-nologo', '-manifest', manifestPath, `-outputresource:${exePath};#1`],
    { encoding: 'utf8' },
  );
  if (res.status !== 0) {
    throw new Error(
      `mt.exe failed to embed Windows app manifest (${res.stderr?.trim() || res.status})`,
    );
  }
}

function fmt(num) {
  return Number(num).toFixed(4);
}

function printMarkdownSummary(report) {
  console.log(`# Knowledge Base Retrieval Eval (${report.mode.toUpperCase()})`);
  console.log('');
  console.log(
    `- Corpus: ${report.corpusDocs} docs / ${report.totalChunks} chunks | Queries: ${report.totalQueries} | Zero-keyword-overlap: ${report.zeroOverlapQueries}/${report.totalQueries} (${(report.zeroOverlapRatio * 100).toFixed(1)}%)`,
  );
  console.log(
    `- Integrity: missingTargets=${report.missingQrelTargets.length}, verbatimSubstrings=${report.verbatimSubstringQueries.length}`,
  );
  console.log('');
  console.log('## Aggregate Metrics');
  console.log('');
  console.log('| Slice | Queries | Recall@8 | MRR@10 | nDCG@10 |');
  console.log('| --- | ---: | ---: | ---: | ---: |');
  console.log(
    `| **Overall** | ${report.totalQueries} | **${fmt(report.recallAt8)}** | **${fmt(report.mrrAt10)}** | **${fmt(report.ndcgAt10)}** |`,
  );
  for (const [cat, stats] of Object.entries(report.byCategory)) {
    console.log(
      `| category:${cat} | ${stats.count} | ${fmt(stats.recallAt8)} | ${fmt(stats.mrrAt10)} | ${fmt(stats.ndcgAt10)} |`,
    );
  }
  for (const [lang, stats] of Object.entries(report.byLanguage)) {
    console.log(
      `| lang:${lang} | ${stats.count} | ${fmt(stats.recallAt8)} | ${fmt(stats.mrrAt10)} | ${fmt(stats.ndcgAt10)} |`,
    );
  }
  console.log('');
  console.log('## Per-Query Results');
  console.log('');
  console.log('| ID | Cat | Lang | ZeroOverlap | Rank | R@8 | MRR@10 | nDCG@10 | Top-1 Hit | Query |');
  console.log('| --- | --- | --- | :---: | ---: | ---: | ---: | ---: | --- | --- |');
  for (const row of report.queries) {
    const rankStr = row.firstRelevantRank ? `#${row.firstRelevantRank}` : 'MISS';
    const top1 = row.hits[0]
      ? `${row.hits[0].file} (${row.hits[0].relevant ? 'HIT' : 'other'})`
      : '—';
    const shortQuery = row.query.replace(/\|/g, '\\|');
    console.log(
      `| ${row.id} | ${row.category} | ${row.language} | ${row.zeroTokenOverlap ? 'yes' : 'no'} | ${rankStr} | ${fmt(row.recallAt8)} | ${fmt(row.mrrAt10)} | ${fmt(row.ndcgAt10)} | ${top1} | ${shortQuery} |`,
    );
  }
}

function main() {
  const opts = parseArgs(process.argv.slice(2));
  const exePath = buildTestBinary();
  ensureWindowsManifest(exePath);

  const tmpOut = path.join(
    os.tmpdir(),
    `kb-eval-${process.pid}-${Date.now()}.json`,
  );
  try {
    const run = spawnSync(
      exePath,
      ['--exact', TEST_NAME, '--ignored', '--nocapture'],
      {
        cwd: REPO_ROOT,
        encoding: 'utf8',
        env: {
          ...process.env,
          KB_EVAL_CORPUS_DIR: opts.corpus,
          KB_EVAL_QRELS_PATH: opts.qrels,
          KB_EVAL_OUTPUT_JSON: tmpOut,
        },
        maxBuffer: 64 * 1024 * 1024,
      },
    );
    if (run.status !== 0) {
      if (run.stdout) process.stdout.write(run.stdout);
      if (run.stderr) process.stderr.write(run.stderr);
      throw new Error(`Eval runner exited with code ${run.status}`);
    }

    const raw = fs.readFileSync(tmpOut, 'utf8');
    const report = JSON.parse(raw);

    if (opts.out) {
      fs.mkdirSync(path.dirname(opts.out), { recursive: true });
      fs.writeFileSync(opts.out, `${JSON.stringify(report, null, 2)}\n`, 'utf8');
    }

    if (opts.json) {
      process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
    } else {
      printMarkdownSummary(report);
    }

    if (opts.check) {
      if (report.recallAt8 < 0.7 || report.mrrAt10 < 0.5) {
        process.stderr.write(
          `[knowledge-eval] Threshold check failed: Recall@8=${fmt(report.recallAt8)} (need >=0.70), MRR@10=${fmt(report.mrrAt10)} (need >=0.50)\n`,
        );
        process.exit(1);
      }
    }
  } finally {
    try {
      fs.rmSync(tmpOut, { force: true });
    } catch {
      // ignore temp cleanup failure
    }
  }
}

main();
