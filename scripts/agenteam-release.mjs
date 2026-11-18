import { createHash, createPublicKey, verify } from 'node:crypto';
import { createReadStream } from 'node:fs';
import { readFile, writeFile } from 'node:fs/promises';
import { basename, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(import.meta.dirname, '..');
const config = JSON.parse(await readFile(join(root, 'src-tauri/tauri.conf.json'), 'utf8'));
const repository = 'https://github.com/micsnq2yoma-lgtm/AgenTeam';

async function digest(path, algorithm) {
  const hash = createHash(algorithm);
  for await (const chunk of createReadStream(path)) hash.update(chunk);
  return hash.digest();
}

// Minisign wraps an Ed25519 key and key ID in a packet. Modern signatures
// prehash the artifact with BLAKE2b-512. The global signature binds the comment.
export async function verifyInstaller(path, wrappedSignature, wrappedKey) {
  const lines = wrapped => Buffer.from(wrapped.trim(), 'base64').toString('utf8').trimEnd().split(/\r?\n/);
  const key = Buffer.from(lines(wrappedKey)[1], 'base64');
  const [, sigLine, comment, globalLine] = lines(wrappedSignature);
  const sig = Buffer.from(sigLine, 'base64');
  if (key.length !== 42 || sig.length !== 74 || key.toString('ascii', 0, 2) !== 'Ed'
    || !sig.subarray(2, 10).equals(key.subarray(2, 10)) || !comment?.startsWith('trusted comment: ')) {
    throw new Error('Installer signature does not match the AgenTeam signing key');
  }
  const publicKey = createPublicKey({
    key: Buffer.concat([Buffer.from('302a300506032b6570032100', 'hex'), key.subarray(10)]),
    type: 'spki', format: 'der',
  });
  const signature = sig.subarray(10);
  const algorithm = sig.toString('ascii', 0, 2);
  if (!['Ed', 'ED'].includes(algorithm)) throw new Error('Unsupported minisign algorithm');
  const content = algorithm === 'ED' ? await digest(path, 'blake2b512') : await readFile(path);
  const trusted = Buffer.concat([signature, Buffer.from(comment.slice('trusted comment: '.length))]);
  if (!verify(null, content, publicKey, signature) || !verify(null, trusted, publicKey, Buffer.from(globalLine, 'base64'))) {
    throw new Error('Invalid installer signature');
  }
}

export async function prepareRelease(directory) {
  const { version, productName, plugins } = config;
  const pkg = JSON.parse(await readFile(join(root, 'package.json'), 'utf8'));
  if (productName !== 'AgenTeam' || pkg.version !== version || !/^\d+\.\d+\.\d+$/.test(version)
    || plugins.updater.endpoints[0] !== repository + '/releases/latest/download/latest.json') {
    throw new Error('Inconsistent release configuration');
  }
  const name = `AgenTeam_${version}_x64-setup.exe`;
  const path = join(directory, name);
  const signature = (await readFile(path + '.sig', 'utf8')).trim();
  await verifyInstaller(path, signature, plugins.updater.pubkey);
  const manifest = { version, platforms: { 'windows-x86_64': {
    signature, url: `${repository}/releases/download/v${version}/${name}`,
  } } };
  const feed = join(directory, 'latest.json');
  await writeFile(feed, JSON.stringify(manifest, null, 2) + '\n');
  const sums = [];
  for (const file of [path, path + '.sig', feed]) {
    sums.push(`${(await digest(file, 'sha256')).toString('hex')}  ${basename(file)}`);
  }
  await writeFile(join(directory, 'SHA256SUMS'), sums.join('\n') + '\n');
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  if (process.argv.length !== 3) throw new Error('Usage: node scripts/agenteam-release.mjs <signed-installer-directory>');
  await prepareRelease(resolve(process.argv[2]));
}
