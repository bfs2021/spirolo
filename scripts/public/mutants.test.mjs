import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

const script = fileURLToPath(new URL('./mutants.mjs', import.meta.url));

// Runs the wrapper around a stand-in for cargo-mutants: a command that leaves the given
// outcomes.json (as cargo-mutants 27.1.0 writes it) and exits with the given status.
function run({ status, outcomes, command = [process.execPath, '-e', `process.exit(${status})`] }) {
  const cwd = mkdtempSync(join(tmpdir(), 'spirolo-mutants-'));
  if (outcomes) {
    mkdirSync(join(cwd, 'mutants.out'));
    writeFileSync(join(cwd, 'mutants.out/outcomes.json'), JSON.stringify(outcomes));
  }
  const summary = join(cwd, 'summary.md');
  writeFileSync(summary, '');
  const result = spawnSync(process.execPath, [script, 'run', 'spirolo-inputs 0/2', '--', ...command], {
    cwd, encoding: 'utf8', env: { ...process.env, GITHUB_STEP_SUMMARY: summary },
  });
  return { status: result.status, stderr: result.stderr, summary: readFileSync(summary, 'utf8') };
}

const counted = { total_mutants: 223, caught: 10, missed: 206, unviable: 7, timeout: 0, success: 0, outcomes: [] };

test('a run that tested mutants and left survivors is a measurement: the job passes and reports the counts', () => {
  const { status, summary } = run({ status: 2, outcomes: counted });
  assert.equal(status, 0);
  assert.equal(summary, [
    '### Mutants: spirolo-inputs 0/2',
    '',
    '| Tested | Caught | Missed | Unviable | Timeout |',
    '| ---: | ---: | ---: | ---: | ---: |',
    '| 223 | 10 | 206 | 7 | 0 |',
    '',
    'cargo-mutants exited 2: some mutants were missed.',
    '',
  ].join('\n'));
});

test('a run with nothing missed, or with timeouts, is a measurement too', () => {
  const clean = run({ status: 0, outcomes: { ...counted, caught: 216, missed: 0 } });
  assert.equal(clean.status, 0);
  assert.match(clean.summary, /\| 223 \| 216 \| 0 \| 7 \| 0 \|\n\ncargo-mutants exited 0: every viable mutant was caught\.\n$/);
  const slow = run({ status: 3, outcomes: { ...counted, missed: 204, timeout: 2 } });
  assert.equal(slow.status, 0);
  assert.match(slow.summary, /\| 223 \| 10 \| 204 \| 7 \| 2 \|\n\ncargo-mutants exited 3: some mutants timed out\.\n$/);
});

test('a failing baseline measured nothing: the job fails and says so, without counts', () => {
  const { status, stderr, summary } = run({ status: 4, outcomes: { ...counted, total_mutants: 0, caught: 0, missed: 0, unviable: 0 } });
  assert.equal(status, 1);
  const reason = 'Not measured. cargo-mutants exited 4: the tests fail or hang before any mutation.';
  assert.equal(summary, `### Mutants: spirolo-inputs 0/2\n\n${reason}\n`);
  assert.ok(stderr.includes(reason));
});

test('any other exit status is a run that did not measure', () => {
  for (const [code, meaning] of [
    [1, 'usage error'], [5, 'the --in-diff diff does not match the tree'], [6, 'the --in-diff diff is not valid'],
    [70, 'internal error'], [101, 'unknown exit status'],
  ]) {
    const { status, summary } = run({ status: code, outcomes: counted });
    assert.equal(status, 1, `exit ${code}`);
    assert.ok(summary.endsWith(`\nNot measured. cargo-mutants exited ${code}: ${meaning}.\n`), summary);
  }
});

test('a passing exit status with no mutant tested is not a measurement', () => {
  const empty = run({ status: 0, outcomes: { ...counted, total_mutants: 0, caught: 0, missed: 0, unviable: 0 } });
  assert.equal(empty.status, 1);
  assert.ok(empty.summary.endsWith('\nNot measured. cargo-mutants exited 0 and tested no mutant.\n'), empty.summary);
  const absent = run({ status: 2 });
  assert.equal(absent.status, 1);
  assert.ok(absent.summary.endsWith('\nNot measured. cargo-mutants exited 2 and left no readable mutants.out/outcomes.json.\n'), absent.summary);
});

test('a command that cannot start did not measure', () => {
  const { status, stderr, summary } = run({ command: ['no-such-cargo'] });
  assert.equal(status, 1);
  assert.match(summary, /\nNot measured\. no-such-cargo did not run: /);
  assert.match(stderr, /Not measured\. no-such-cargo did not run: /);
});

test('the plan is one matrix row per shard, for every crate or for the one a manual run names', () => {
  const plan = (crate) => spawnSync(process.execPath, [script, 'plan'], { encoding: 'utf8', env: { ...process.env, CRATE: crate } });
  const all = JSON.parse(plan('').stdout.replace(/^matrix=/, '')).include;
  assert.ok(plan('').stdout.startsWith('matrix='), 'a line the plan job appends to $GITHUB_OUTPUT');
  for (const { crate, shards } of all) {
    // All shards of a crate must agree on the denominator, or they would not partition its mutants.
    assert.deepEqual(all.filter(row => row.crate === crate).map(row => [row.shard, row.shards]),
      Array.from({ length: shards }, (_, shard) => [shard, shards]), crate);
  }
  const one = JSON.parse(plan('spirolo-inputs').stdout.replace(/^matrix=/, '')).include;
  assert.deepEqual(one, all.filter(row => row.crate === 'spirolo-inputs'));
  assert.ok(one.length > 0);

  const unknown = plan('spirolo-bench');
  assert.equal(unknown.status, 1);
  assert.match(unknown.stderr, /spirolo-bench is not a mutated crate \(spirolo, /);
});
