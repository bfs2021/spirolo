// Tag releases are rehearsals unless the public Actions job explicitly enables publishing.
import { readFileSync, realpathSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { distTag, takesLatest } from './dist-tag.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const readPackage = platform => JSON.parse(readFileSync(resolve(root, 'npm', platform, 'package.json'), 'utf8'));
const launcher = readPackage('spirolo');
const registry = 'https://registry.npmjs.org';

export function publishPlan({ ref, directory = 'target/rc', publish = false, actions = false, repositoryPrivate = true } = {}) {
  if (ref !== `refs/tags/v${launcher.version}`) {
    throw new Error(`Release tag must be refs/tags/v${launcher.version}; got ${ref}`);
  }
  if (publish && (!actions || repositoryPrivate)) {
    throw new Error('Publishing requires an explicitly enabled public GitHub Actions tag release');
  }
  const packages = ['linux-x64', 'linux-arm64', 'darwin-x64', 'darwin-arm64'].map(readPackage);
  for (const pkg of packages) {
    if (pkg.version !== launcher.version || launcher.optionalDependencies[pkg.name] !== pkg.version) {
      throw new Error(`Release metadata mismatch: ${pkg.name}`);
    }
  }
  return [...packages, launcher].map(pkg => {
    const filename = `${pkg.name.replace(/^@/, '').replace('/', '-')}-${pkg.version}.tgz`;
    return {
      name: pkg.name, filename,
      args: ['publish', resolve(directory, filename), '--registry', registry,
        '--access', 'public', '--tag', distTag(pkg.version), '--ignore-scripts', '--provenance',
        ...(publish ? [] : ['--dry-run'])],
    };
  });
}

// The `npm dist-tag` commands that point `latest` at a just-published prerelease, one per package,
// native packages before the launcher. Empty for a release, and once any release exists.
// `publishedVersions` answers a package name with every version the registry holds for it.
export function latestPlan({ ref, publishedVersions }) {
  return publishPlan({ ref }).map(({ name }) => name).filter(name => {
    const published = publishedVersions(name);
    if (!published.includes(launcher.version)) throw new Error(`${name}@${launcher.version} is not on the registry yet`);
    return takesLatest(launcher.version, published);
  }).map(name => ({
    name,
    args: ['dist-tag', 'add', `${name}@${launcher.version}`, 'latest', '--registry', registry],
  }));
}

// The registry lists a version some minutes after `npm publish` returns. Reads `read(name)` until
// it holds `version`, pausing between attempts, and answers the last reading either way: the
// caller decides what a version that never appeared means.
export function awaitVersion(name, version, { read, pause, attempts = 40 }) {
  let published = read(name);
  for (let attempt = 1; attempt < attempts && !published.includes(version); attempt += 1) {
    pause();
    published = read(name);
  }
  return published;
}

function registryVersions(name) {
  const result = spawnSync('npm', ['view', name, 'versions', '--json', '--prefer-online', '--registry', registry], { encoding: 'utf8' });
  if (result.status !== 0) throw new Error(`npm view failed: ${name} (${result.error ?? result.stderr})`);
  return [JSON.parse(result.stdout)].flat();
}

export function verifyChecksums(directory, requiredFiles) {
  const manifest = readFileSync(resolve(directory, 'SHA256SUMS'), 'utf8').trim();
  const seen = new Set();
  for (const line of manifest.split('\n')) {
    const match = /^([a-f0-9]{64})  ([A-Za-z0-9][A-Za-z0-9._-]*)$/.exec(line);
    if (!match) throw new Error(`Invalid checksum line: ${line}`);
    const [, expected, filename] = match;
    if (seen.has(filename)) throw new Error(`Duplicate checksum: ${filename}`);
    seen.add(filename);
    const actual = createHash('sha256').update(readFileSync(resolve(directory, filename))).digest('hex');
    if (actual !== expected) throw new Error(`Checksum mismatch: ${filename}`);
  }
  for (const filename of requiredFiles) {
    if (!seen.has(filename)) throw new Error(`Missing checksum: ${filename}`);
  }
}

// Node names this module by its real path; resolve a symlinked launch path the same way.
if (process.argv[1] && realpathSync(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const checkTag = process.argv[2] === '--check-tag';
  const latest = process.argv[2] === '--latest';
  const directory = resolve(checkTag || latest ? 'target/rc' : process.argv[2] ?? 'target/rc');
  const plan = publishPlan({
    ref: process.env.GITHUB_REF,
    directory,
    publish: process.env.PUBLISH_NPM === 'true',
    actions: process.env.GITHUB_ACTIONS === 'true',
    repositoryPrivate: process.env.REPOSITORY_PRIVATE !== 'false',
  });
  if (checkTag) {
    console.log(`Release tag matches ${launcher.version}`);
  } else if (latest) {
    // After publication only: the registry is read for what it holds, then `latest` is moved.
    if (process.env.PUBLISH_NPM !== 'true') throw new Error('Moving latest requires an enabled publication');
    // Up to ten minutes per package for the registry to list the version just published.
    const pause = () => Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 15_000);
    const moves = latestPlan({
      ref: process.env.GITHUB_REF,
      publishedVersions: name => awaitVersion(name, launcher.version, { read: registryVersions, pause }),
    });
    if (moves.length === 0) console.log(`latest stays where it is for ${launcher.version}`);
    for (const { name, args } of moves) {
      console.log(`Point latest at ${name}@${launcher.version}`);
      const result = spawnSync('npm', args, { stdio: 'inherit' });
      if (result.status !== 0) throw new Error(`npm dist-tag failed: ${name} (${result.error ?? result.status})`);
    }
  } else {
    // Validate everything before publishing the first package.
    verifyChecksums(directory, plan.map(item => item.filename));
    for (const { name, args } of plan) {
      console.log(`${args.includes('--dry-run') ? 'Rehearse' : 'Publish'} ${name}`);
      const result = spawnSync('npm', args, { stdio: 'inherit' });
      if (result.status !== 0) throw new Error(`npm publish failed: ${name} (${result.error ?? result.status})`);
    }
  }
}
