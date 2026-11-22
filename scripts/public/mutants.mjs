// The weekly mutation run (.github/workflows/mutants.yml).
//
//   CRATE=<crate or empty> node scripts/public/mutants.mjs plan
//   node scripts/public/mutants.mjs run <title> -- cargo mutants ...
//   CRATE=<crate or empty> node scripts/public/mutants.mjs totals <directory>
//
// `plan` prints the job matrix as a `matrix=` line for $GITHUB_OUTPUT: one row per shard.
// `run` executes cargo-mutants, appends the counts it left in mutants.out/outcomes.json to
// the job summary, and exits 0 whenever mutants were tested, survivors or not. A survivor
// is a number to report; a run that tested nothing is the failure.
// `totals` sums the shards' outcomes.json files, one directory per downloaded artifact, into
// one job-summary row per crate.
import { spawnSync } from 'node:child_process';
import { appendFileSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

// Shards per mutated crate, about 120 mutants each. Measured on the first full run
// (2026-11-21, 44 shards, 2,538 runner minutes): a mutant costs 9 to 25 seconds in five
// crates and 42 in spirolo-model (rebuild and relink the workspace's tests, then run them),
// so a shard took 14 to 97 minutes, well inside the six hours a job may take. Each shard
// spends 67 to 104 seconds outside the mutation step (setup, the baseline, the upload), 61
// minutes or 2.4% of the run in all, so fewer, larger shards would save almost nothing; what keeps the run out of the way of other jobs
// is `max-parallel` in the workflow, not this table. Recount with
// `cargo mutants --list --package <crate> | wc -l` (2026-11: 323, 558, 223, 2215, 569, 1201).
const SHARDS = {
  spirolo: 3,
  'spirolo-frameworks': 5,
  'spirolo-inputs': 2,
  'spirolo-model': 19,
  'spirolo-report': 5,
  'spirolo-rules': 10,
};

export function shardPlan(only = '') {
  if (only && !Object.hasOwn(SHARDS, only)) {
    throw new Error(`${only} is not a mutated crate (${Object.keys(SHARDS).join(', ')})`);
  }
  return Object.entries(SHARDS).filter(([crate]) => !only || crate === only)
    .flatMap(([crate, shards]) => Array.from({ length: shards }, (_, shard) => ({ crate, shard, shards })));
}

// cargo-mutants 27.1.0 exit statuses (https://mutants.rs/exit-codes.html, src/exit_code.rs).
// Under 0, 2 and 3 the mutants were tested; under every other status they were not.
const MEASURED = new Map([
  [0, 'every viable mutant was caught'],
  [2, 'some mutants were missed'],
  [3, 'some mutants timed out'],
]);
const NOT_MEASURED = new Map([
  [1, 'bad arguments or another error'],
  [4, 'the tests fail or hang before any mutation'],
  [5, 'the --in-diff diff does not match the tree'],
  [6, 'the --in-diff diff is not valid'],
  [70, 'internal error'],
]);

// Runs the command: its counts when it measured, or why it measured nothing.
function outcome(command) {
  const { status, signal, error } = spawnSync(command[0], command.slice(1), { stdio: 'inherit' });
  if (error) return { failure: `${command[0]} did not run: ${error.message}.` };
  if (signal) return { failure: `cargo-mutants was killed by ${signal}.` };
  if (!MEASURED.has(status)) {
    return { failure: `cargo-mutants exited ${status}: ${NOT_MEASURED.get(status) ?? 'unknown exit status'}.` };
  }
  let counts;
  try {
    counts = JSON.parse(readFileSync('mutants.out/outcomes.json', 'utf8'));
  } catch {
    return { failure: `cargo-mutants exited ${status} and left no readable mutants.out/outcomes.json.` };
  }
  const empty = unmeasured(counts);
  if (empty) return { failure: `cargo-mutants exited ${status} and ${empty}.` };
  return { status, counts };
}

// Why counts that cargo-mutants wrote are still not a measurement.
function unmeasured(counts) {
  if (!(counts.total_mutants > 0)) return 'tested no mutant';
  // Every build failed (a full disk, say), so no mutant reached the tests.
  if (counts.unviable === counts.total_mutants) return 'every mutant was unviable';
  return undefined;
}

function run(title, command) {
  const { failure, status, counts } = outcome(command);
  const lines = failure ? [`Not measured. ${failure}`] : [
    '| Tested | Caught | Missed | Unviable | Timeout |',
    '| ---: | ---: | ---: | ---: | ---: |',
    `| ${counts.total_mutants} | ${counts.caught} | ${counts.missed} | ${counts.unviable} | ${counts.timeout} |`,
    '',
    `cargo-mutants exited ${status}: ${MEASURED.get(status)}.`,
  ];
  report(`### Mutants: ${title}\n\n${lines.join('\n')}\n`, failure && lines[0]);
}

const COUNTS = ['total_mutants', 'caught', 'missed', 'unviable', 'timeout'];

function totals(directory, only) {
  const rows = [];
  for (const { crate, shard, shards } of shardPlan(only)) {
    let row = rows.find((candidate) => candidate.crate === crate);
    if (!row) rows.push(row = { crate, reported: 0, shards, counts: COUNTS.map(() => 0) });
    let counts;
    try {
      counts = JSON.parse(readFileSync(`${directory}/mutants-${crate}-${shard}-of-${shards}/outcomes.json`, 'utf8'));
    } catch {
      continue;
    }
    // Without an end time the shard was stopped part-way, and its counts are partial.
    if (!counts.end_time || unmeasured(counts)) continue;
    row.reported += 1;
    row.counts = row.counts.map((sum, index) => sum + counts[COUNTS[index]]);
  }
  const sum = (pick) => rows.reduce((total, row) => total + pick(row), 0);
  rows.push({
    crate: '**Total**', reported: sum((row) => row.reported), shards: sum((row) => row.shards),
    counts: COUNTS.map((_, index) => sum((row) => row.counts[index])),
  });
  const shards = (row) => `${row.reported}/${row.shards}${row.reported < row.shards ? ' (incomplete)' : ''}`;
  const lines = [
    '| Crate | Shards | Tested | Caught | Missed | Unviable | Timeout |',
    '| :--- | ---: | ---: | ---: | ---: | ---: | ---: |',
    ...rows.map((row) => `| ${row.crate} | ${shards(row)} | ${row.counts.join(' | ')} |`),
  ];
  const { reported, shards: planned } = rows.at(-1);
  const failure = reported < planned
    && `Incomplete: ${planned - reported} of ${planned} shards did not finish measuring, and their mutants are not counted.`;
  if (failure) lines.push('', failure);
  report(`### Mutants per crate\n\n${lines.join('\n')}\n`, failure);
}

// To the job summary on GitHub, with a failure repeated in the log; to stdout elsewhere.
function report(summary, failure) {
  if (process.env.GITHUB_STEP_SUMMARY) {
    appendFileSync(process.env.GITHUB_STEP_SUMMARY, summary);
    if (failure) console.error(failure);
  } else process.stdout.write(summary);
  if (failure) process.exitCode = 1;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  try {
    const [mode, title, separator, ...command] = process.argv.slice(2);
    if (mode === 'plan') console.log(`matrix=${JSON.stringify({ include: shardPlan(process.env.CRATE) })}`);
    else if (mode === 'run' && separator === '--' && command.length) run(title, command);
    else if (mode === 'totals' && title) totals(title, process.env.CRATE);
    else throw new Error('usage: node scripts/public/mutants.mjs plan | run <title> -- <command...> | totals <directory>');
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
