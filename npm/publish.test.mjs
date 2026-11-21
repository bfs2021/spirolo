import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { awaitVersion, latestPlan, publishPlan, verifyChecksums } from './publish.mjs';
import { distTag } from './dist-tag.mjs';

const launcher = JSON.parse(readFileSync(new URL('./spirolo/package.json', import.meta.url)));
const ref = `refs/tags/v${launcher.version}`;

test('default publication rehearses all native packages before the launcher with provenance', () => {
  const plan = publishPlan({ ref, directory: 'target/rc' });
  assert.equal(plan.length, Object.keys(launcher.optionalDependencies).length + 1);
  assert.equal(plan.at(-1).name, launcher.name);
  assert.deepEqual(new Set(plan.slice(0, -1).map(item => item.name)), new Set(Object.keys(launcher.optionalDependencies)));
  for (const { args } of plan) {
    assert.ok(args.includes('--dry-run'));
    assert.ok(args.includes('--provenance'));
    assert.ok(args.includes('--ignore-scripts'));
    assert.equal(args[args.indexOf('--tag') + 1], distTag(launcher.version));
    assert.equal(args[args.indexOf('--registry') + 1], 'https://registry.npmjs.org');
  }
});

test('only an enabled public Actions tag can publish; mismatched tags fail before npm', () => {
  assert.throws(() => publishPlan({ ref: 'refs/heads/main' }), /tag/);
  assert.throws(() => publishPlan({ ref: 'refs/tags/v99.0.0' }), /tag/);
  assert.throws(() => publishPlan({ ref, publish: true }), /public GitHub Actions/);
  assert.throws(() => publishPlan({ ref, publish: true, actions: true, repositoryPrivate: true }), /public GitHub Actions/);
  const plan = publishPlan({ ref, publish: true, actions: true, repositoryPrivate: false });
  assert.ok(plan.every(({ args }) => !args.includes('--dry-run') && args.includes('--provenance')));
});

test('after publication latest is pointed at every package of a prerelease, natives first, unless a release exists', () => {
  const names = [...Object.keys(launcher.optionalDependencies), launcher.name];
  const prerelease = launcher.version.includes('-');
  const moves = latestPlan({ ref, publishedVersions: () => ['0.0.1-reserved.0', launcher.version] });
  assert.deepEqual(moves.map(move => move.name), prerelease ? names : []);
  for (const { name, args } of moves) {
    assert.deepEqual(args.slice(0, 4), ['dist-tag', 'add', `${name}@${launcher.version}`, 'latest']);
    assert.equal(args[args.indexOf('--registry') + 1], 'https://registry.npmjs.org');
  }
  // A package that already has a release keeps its `latest`; the others still move.
  const mixed = latestPlan({ ref, publishedVersions: name => name === launcher.name
    ? ['0.0.9', launcher.version] : [launcher.version] });
  assert.ok(!mixed.some(move => move.name === launcher.name));
  // The tag is never moved to a version the registry does not hold, nor from a mismatched tag.
  assert.throws(() => latestPlan({ ref, publishedVersions: () => ['0.0.1-reserved.0'] }), /not on the registry yet/);
  assert.throws(() => latestPlan({ ref: 'refs/tags/v99.0.0', publishedVersions: () => [] }), /tag/);
});

test('the registry is read again until it lists the published version, and not forever', () => {
  const readings = [['0.1.0-alpha.1'], ['0.1.0-alpha.1'], ['0.1.0-alpha.1', '0.1.0-alpha.2']];
  let reads = 0;
  let pauses = 0;
  const read = () => readings[Math.min(reads++, readings.length - 1)];
  const pause = () => { pauses += 1; };
  assert.deepEqual(awaitVersion('spirolo', '0.1.0-alpha.2', { read, pause }), readings[2]);
  assert.deepEqual([reads, pauses], [3, 2]);
  // Already listed: one read, no pause.
  reads = 2; pauses = 0;
  awaitVersion('spirolo', '0.1.0-alpha.2', { read, pause });
  assert.deepEqual([reads, pauses], [3, 0]);
  // Never listed: it stops after its attempts and hands back what the registry holds.
  let never = 0;
  const stale = awaitVersion('spirolo', '9.9.9', { read: () => { never += 1; return ['0.1.0-alpha.1']; }, pause, attempts: 5 });
  assert.deepEqual([stale, never], [['0.1.0-alpha.1'], 5]);
});

test('a real npm pack passes checksums and provenance dry run; a modified tarball is refused', () => {
  const directory = mkdtempSync(join(tmpdir(), 'spirolo-publish-'));
  try {
    const pack = spawnSync('npm', ['pack', './npm/spirolo', '--json', '--ignore-scripts', '--pack-destination', directory], { encoding: 'utf8' });
    assert.equal(pack.status, 0, pack.stderr);
    const [packed] = JSON.parse(pack.stdout);
    const tarball = join(directory, packed.filename);
    const digest = createHash('sha256').update(readFileSync(tarball)).digest('hex');
    writeFileSync(join(directory, 'SHA256SUMS'), `${digest}  ${packed.filename}\n`);
    verifyChecksums(directory, [packed.filename]);
    assert.throws(() => verifyChecksums(directory, ['missing.tgz']), /Missing checksum/);
    const command = publishPlan({ ref, directory }).at(-1);
    const result = spawnSync('npm', [...command.args, '--offline'], { encoding: 'utf8' });
    assert.equal(result.status, 0, result.stderr);
    assert.match(result.stderr, /dry-run/);
    writeFileSync(tarball, 'corrupted');
    assert.throws(() => verifyChecksums(directory, [packed.filename]), /Checksum mismatch/);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});
