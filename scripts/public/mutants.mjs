// The weekly mutation run (.github/workflows/mutants.yml).
//
//   CRATE=<crate or empty> node scripts/public/mutants.mjs plan
//   node scripts/public/mutants.mjs run <title> -- cargo mutants ...
//
// `plan` prints the job matrix as a `matrix=` line for $GITHUB_OUTPUT: one row per shard.
// `run` executes cargo-mutants, appends the counts it left in mutants.out/outcomes.json to
// the job summary, and exits 0 whenever mutants were tested, survivors or not. A survivor
// is a number to report; a run that tested nothing is the failure.
import { spawnSync } from 'node:child_process';
import { appendFileSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

// Shards per mutated crate, sized for about 120 mutants each: on a GitHub runner a mutant
// costs up to a minute (rebuild and relink the workspace's tests, then run them), so a shard
// should end in about two hours, well inside the six a job may take. Recount with
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
  [1, 'usage error'],
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
  if (!(counts.total_mutants > 0)) return { failure: `cargo-mutants exited ${status} and tested no mutant.` };
  return { status, counts };
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
  const summary = `### Mutants: ${title}\n\n${lines.join('\n')}\n`;
  if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY, summary);
  else process.stdout.write(summary);
  if (failure) {
    console.error(`Not measured. ${failure}`);
    process.exitCode = 1;
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  try {
    const [mode, title, separator, ...command] = process.argv.slice(2);
    if (mode === 'plan') console.log(`matrix=${JSON.stringify({ include: shardPlan(process.env.CRATE) })}`);
    else if (mode === 'run' && separator === '--' && command.length) run(title, command);
    else throw new Error('usage: node scripts/public/mutants.mjs plan | run <title> -- <command...>');
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
