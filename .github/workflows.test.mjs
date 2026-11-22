import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { test } from 'node:test';
import { parseDocument } from 'yaml';

const workflows = new URL('./workflows/', import.meta.url);
const documents = readdirSync(workflows).filter(name => /\.ya?ml$/.test(name)).map(name => {
  const source = readFileSync(new URL(name, workflows), 'utf8');
  const document = parseDocument(source, { uniqueKeys: true });
  assert.deepEqual(document.errors, [], name);
  return { name, source, workflow: document.toJS() };
});

test('every workflow pins actions, limits token permissions and discards checkout credentials', () => {
  assert.ok(documents.length > 0);
  for (const { name, source, workflow } of documents) {
    assert.deepEqual(workflow.permissions, { contents: 'read' }, name);
    for (const [jobName, job] of Object.entries(workflow.jobs)) {
      for (const step of job.steps ?? []) {
        if (!step.uses) continue;
        assert.match(step.uses, /^[\w.-]+\/[\w./-]+@[a-f0-9]{40}$/, `${name}/${jobName}: ${step.uses}`);
        const line = source.split('\n').find(line => line.includes(`uses: ${step.uses}`));
        assert.match(line, / # (?:v\d|stable|1\.90)/, `${name}: version comment`);
        if (step.uses.startsWith('actions/checkout@')) {
          assert.equal(step.with?.['persist-credentials'], false, `${name}/${jobName}`);
        }
      }
      const permissions = job.permissions ?? workflow.permissions;
      const expected = name === 'release.yml' && jobName === 'publish'
        ? { contents: 'write', 'id-token': 'write' }
        : name === 'release.yml' && ['registry', 'latest'].includes(jobName) ? { contents: 'read', 'id-token': 'write' }
        : name === 'codeql.yml' ? { contents: 'read', 'security-events': 'write' } : { contents: 'read' };
      assert.deepEqual(permissions, expected, `${name}/${jobName}: token permissions`);
    }
  }
});

test('a v tag builds all npm platforms and rehearses before explicitly enabled publication', () => {
  const release = documents.find(({ name }) => name === 'release.yml').workflow;
  assert.deepEqual(release.on, { push: { tags: ['v*'] } });
  assert.deepEqual(release.jobs.build.strategy.matrix.include.map(row => row.target).sort(), [
    'aarch64-apple-darwin', 'aarch64-unknown-linux-gnu',
    'x86_64-apple-darwin', 'x86_64-unknown-linux-gnu',
  ]);
  assert.equal(release.jobs.package.needs, 'build');
  assert.ok(release.jobs.package.steps.some(step => step.run === 'node npm/stage.mjs target/rc'));
  assert.ok(release.jobs.package.steps.some(step => step.run === 'sha256sum --check SHA256SUMS'));
  assert.ok(release.jobs.package.steps.some(step => step.run === 'node npm/publish.mjs target/rc'));
  const publish = release.jobs.publish;
  assert.equal(publish.needs, 'package');
  assert.equal(publish.if, "vars.RELEASE_PUBLISH == 'true' && github.event.repository.private == false");
  assert.deepEqual(publish.permissions, { contents: 'write', 'id-token': 'write' });
  assert.equal(publish.env.PUBLISH_NPM, 'true');
  assert.equal(publish.env.REPOSITORY_PRIVATE, '${{ github.event.repository.private }}');
  assert.ok(publish.steps.some(step => step.run === 'node npm/publish.mjs target/rc'));
  assert.ok(publish.steps.some(step => step.run?.includes('gh release create')));
  // Release text comes from the version's CHANGELOG section, never from generated PR titles.
  assert.ok(publish.steps.some(step => step.run?.includes('--notes-file') && step.run.includes('npm/release-notes.mjs')));
  assert.ok(!publish.steps.some(step => step.run?.includes('--generate-notes')));
  assert.ok(release.jobs.package.steps.some(step =>
    step.uses?.startsWith('actions/upload-artifact@') && step.with.path === 'target/rc/*'));
});

test('latest is moved only after an enabled publication, by a pinned npm and the workflow identity', () => {
  const { latest, publish } = documents.find(({ name }) => name === 'release.yml').workflow.jobs;
  assert.equal(latest.needs, 'publish');
  assert.equal(latest.if, publish.if);
  assert.equal(latest.env.PUBLISH_NPM, 'true');
  // Without it the publish script reads the repository as private and refuses to run.
  assert.equal(latest.env.REPOSITORY_PRIVATE, publish.env.REPOSITORY_PRIVATE);
  assert.match(latest.env.NPM_VERSION, /^\d+\.\d+\.\d+$/);
  const install = latest.steps.findIndex(step => step.run?.includes('npm install --global "npm@${NPM_VERSION}"'));
  const move = latest.steps.findIndex(step => step.run === 'node npm/publish.mjs --latest');
  assert.ok(install >= 0 && move > install, 'the pinned npm is installed before it moves the tag');
  assert.ok(latest.steps[install].run.includes('--ignore-scripts'));
  assert.ok(!JSON.stringify(latest).includes('secrets.'), 'no stored token');
});

test('the MCP Registry listing follows the npm publication, from a verified pinned publisher', () => {
  const { registry, publish } = documents.find(({ name }) => name === 'release.yml').workflow.jobs;
  assert.equal(registry.needs, 'publish');
  assert.equal(registry.if, publish.if);
  assert.match(registry.env.MCP_PUBLISHER_VERSION, /^v\d+\.\d+\.\d+$/);
  const verify = registry.steps.findIndex(step => step.run?.includes('cosign verify-blob'));
  const submit = registry.steps.findIndex(step => step.run?.includes('mcp-publisher" publish'));
  assert.ok(verify >= 0 && submit > verify, 'the publisher is verified before it runs');
  assert.ok(!registry.steps.some(step => step.run?.includes('releases/latest')), 'never the mutable latest URL');
  assert.ok(registry.steps[submit].run.includes('login github-oidc'), 'no stored token');
  // The registry reads `mcpName` from the npm version server.json names, so the three must agree.
  const server = JSON.parse(readFileSync(new URL('../server.json', import.meta.url), 'utf8'));
  const launcher = JSON.parse(readFileSync(new URL('../npm/spirolo/package.json', import.meta.url), 'utf8'));
  assert.equal(server.name, launcher.mcpName);
  assert.deepEqual([server.version, server.packages.map(pkg => `${pkg.identifier}@${pkg.version}`)],
    [launcher.version, [`${launcher.name}@${launcher.version}`]]);
  assert.ok(server.description.length <= 100, 'the registry refuses a description over 100 characters');
});

test('native artifacts come from locked release builds for the selected matrix target', () => {
  const { jobs } = documents.find(({ name }) => name === 'release.yml').workflow;
  assert.equal(jobs.build['runs-on'], '${{ matrix.os }}');
  const steps = jobs.build.steps;
  const buildIndex = steps.findIndex(step => /^cargo build\b/.test(step.run ?? ''));
  assert.ok(buildIndex >= 0, 'native compilation must execute cargo build');
  const build = steps[buildIndex];
  assert.equal(build.env.TARGET, '${{ matrix.target }}');
  for (const argument of ['--release', '--locked', '-p spirolo', '--target "$TARGET"']) {
    assert.ok(build.run.includes(argument), `cargo build requires ${argument}`);
  }
  const archiveIndex = steps.findIndex(step => /^\s*tar -cJf\b/m.test(step.run ?? ''));
  assert.ok(archiveIndex > buildIndex, 'archive creation must execute tar after compilation');
  const archive = steps[archiveIndex];
  assert.equal(archive.env.TARGET, '${{ matrix.target }}');
  assert.match(archive.run, /^\s*cp "target\/\$TARGET\/release\/spirolo" "target\/distrib\/spirolo-\$TARGET\/"$/m);
  assert.match(archive.run, /^\s*tar -cJf "target\/distrib\/spirolo-\$TARGET\.tar\.xz" -C target\/distrib "spirolo-\$TARGET"$/m);
  const uploadIndex = steps.findIndex(step => step.uses?.startsWith('actions/upload-artifact@'));
  assert.ok(uploadIndex > archiveIndex, 'upload must follow archive creation');
  assert.equal(steps[uploadIndex].with.path, 'target/distrib/*.tar.xz');
  assert.equal(steps[uploadIndex].with['if-no-files-found'], 'error');
});

test('CodeQL scans Actions, JavaScript/TypeScript and Rust on main, pull requests and weekly', () => {
  const codeql = documents.find(({ name }) => name === 'codeql.yml').workflow;
  assert.deepEqual(Object.keys(codeql.on).sort(), ['pull_request', 'push', 'schedule']);
  assert.deepEqual(codeql.on.push, { branches: ['main'] });
  assert.equal(codeql.on.schedule.length, 1);
  const { analyze } = codeql.jobs;
  assert.deepEqual(analyze.strategy.matrix.language, ['actions', 'javascript-typescript', 'rust']);
  const init = analyze.steps.find(step => step.uses?.startsWith('github/codeql-action/init@'));
  const run = analyze.steps.find(step => step.uses?.startsWith('github/codeql-action/analyze@'));
  assert.equal(init.with.languages, '${{ matrix.language }}');
  assert.equal(init.uses.split('@')[1], run.uses.split('@')[1], 'init and analyze share one CodeQL release');
});

test('Dependabot maintains Cargo, npm, Actions and the Rust version every week', () => {
  const document = parseDocument(readFileSync(new URL('./dependabot.yml', import.meta.url), 'utf8'));
  assert.deepEqual(document.errors, []);
  const config = document.toJS();
  assert.equal(config.version, 2);
  assert.deepEqual(config.updates.map(entry => entry['package-ecosystem']).sort(),
    ['cargo', 'github-actions', 'npm', 'rust-toolchain']);
  for (const entry of config.updates) {
    assert.equal(entry.schedule.interval, 'weekly');
    // The Rust version is one dependency: its pull request is already the only one.
    if (entry['package-ecosystem'] === 'rust-toolchain') continue;
    assert.ok(Object.keys(entry.groups ?? {}).length > 0, `${entry['package-ecosystem']} groups`);
  }
  // rust-toolchain.toml sits at the root; without this entry the pinned version never moves.
  assert.equal(config.updates.find(entry => entry['package-ecosystem'] === 'rust-toolchain').directory, '/');
  const npm = config.updates.find(entry => entry['package-ecosystem'] === 'npm');
  for (const directory of ['/.github', '/editors/vscode', '/website', '/npm/*']) {
    assert.ok(npm.directories.includes(directory), `npm coverage: ${directory}`);
  }
  const launcher = JSON.parse(readFileSync(new URL('../npm/spirolo/package.json', import.meta.url), 'utf8'));
  assert.ok(npm.ignore.some(entry => entry['dependency-name'] === `${launcher.name}*`), 'native versions follow Cargo');
});

const toolchain = readFileSync(new URL('../rust-toolchain.toml', import.meta.url), 'utf8');

test('every job that runs Rust installs the one version pinned in rust-toolchain.toml', () => {
  // A channel like `stable` moves on its own: a new Rust release then fails clippy, inside the
  // required check, on code nobody touched. An exact version moves only by pull request.
  assert.match(toolchain, /^channel = "\d+\.\d+\.\d+"$/m, 'an exact Rust version');
  let installs = 0;
  for (const { name, source, workflow } of documents) {
    // Nothing in a workflow may choose a compiler: the file is the only place a version is written.
    assert.doesNotMatch(source, /rust-toolchain@|rustup default|rustup override|RUSTUP_TOOLCHAIN|cargo \+|toolchain:/, name);
    for (const [jobName, job] of Object.entries(workflow.jobs)) {
      const steps = job.steps ?? [];
      const rust = steps.findIndex(step =>
        /\b(?:cargo|just)\b/.test(step.run ?? '') || step.uses?.startsWith('Swatinem/rust-cache@'));
      if (rust < 0) continue;
      // With no argument, rustup installs the toolchain the file names, with its components.
      const install = steps.findIndex(step => step.run?.split('\n')[0] === 'rustup toolchain install');
      assert.ok(install >= 0 && install < rust, `${name}/${jobName}: installs the pinned toolchain before using Rust`);
      installs += 1;
    }
  }
  assert.equal(installs, 3, 'the required check, the mutation shards and the release build');
});

test('the required check formats, lints, spell-checks and audits dependencies, with its tools installed', () => {
  // `just ci` is the whole required check, so a gate missing from the recipe is a gate nobody runs.
  const recipes = new Map();
  let body;
  for (const line of readFileSync(new URL('../justfile', import.meta.url), 'utf8').split('\n')) {
    const head = line.match(/^([a-z][\w-]*):(.*)$/);
    if (head) recipes.set(head[1], { needs: head[2].trim().split(/\s+/).filter(Boolean), runs: body = [] });
    else if (/^\s+\S/.test(line)) body?.push(line.trim());
  }
  const runs = recipes.get('ci').needs.flatMap(name => recipes.get(name).runs);
  for (const gate of [
    'cargo fmt --all --check',
    'cargo clippy --locked --workspace --all-targets --all-features -- -D warnings',
    'typos',
    'cargo deny check',
  ]) assert.ok(runs.includes(gate), `just ci runs: ${gate}`);

  const { steps } = documents.find(({ name }) => name === 'ci.yml').workflow.jobs.snapshot;
  assert.equal(steps.at(-1).run, 'just ci');
  const listed = (action, input) => steps.find(step => step.uses?.startsWith(`${action}@`)).with[input].split(/,\s*/);
  // The gates need clippy and rustfmt; rustup installs what rust-toolchain.toml lists.
  const components = JSON.parse(toolchain.match(/^components = (\[.*\])$/m)[1]);
  for (const component of ['clippy', 'rustfmt']) {
    assert.ok(components.includes(component), `toolchain component: ${component}`);
  }
  for (const tool of ['cargo-deny', 'cargo-nextest', 'just', 'typos']) {
    assert.ok(listed('taiki-e/install-action', 'tool').includes(tool), `runner tool: ${tool}`);
  }
});

test('the weekly mutation run measures every mutated crate and fails only when it measured nothing', async () => {
  const { source, workflow } = documents.find(({ name }) => name === 'mutants.yml');
  assert.equal(workflow.on.schedule.length, 1);
  assert.deepEqual(Object.keys(workflow.on.workflow_dispatch.inputs), ['crate']);
  // A swallowed exit status is how this job stayed green while testing no mutant at all.
  assert.doesNotMatch(source, /\|\||continue-on-error|set \+e/);

  const { plan, mutants, totals } = workflow.jobs;
  // The shards come from the committed plan, narrowed to the crate a manual run asks for.
  const planned = plan.steps.find(step => step.run?.includes('scripts/public/mutants.mjs plan'));
  assert.equal(planned.env.CRATE, '${{ inputs.crate }}');
  assert.ok(!planned.run.includes('${{'), 'the input reaches the script as data, never as shell text');
  assert.equal(mutants.needs, 'plan');
  assert.equal(mutants.strategy.matrix, '${{ fromJSON(needs.plan.outputs.matrix) }}');
  assert.equal(mutants.strategy['fail-fast'], false);
  assert.ok(mutants['timeout-minutes'] < 360, 'a shard stops before the runner limit');
  assert.equal(mutants.if, undefined, 'no condition can skip the shards and leave a green run');

  const { shardPlan } = await import('../scripts/public/mutants.mjs');
  const excluded = readFileSync(new URL('../.cargo/mutants.toml', import.meta.url), 'utf8');
  const crates = readdirSync(new URL('../crates/', import.meta.url)).filter(crate => !excluded.includes(`crates/${crate}/**`));
  assert.deepEqual([...new Set(shardPlan().map(row => row.crate))], crates.sort());
  assert.ok(shardPlan('spirolo-model').length > 1, 'the largest crate is split');

  const baseline = mutants.steps.findIndex(step => step.run === 'cargo nextest run --workspace');
  const measure = mutants.steps.findIndex(step => step.run?.includes('cargo mutants'));
  // cargo-mutants 27.1.0 runs only the mutated crate's tests as its baseline, so the workflow proves the suite itself.
  assert.ok(baseline >= 0 && measure > baseline, 'the whole suite passes unmutated before any mutant is judged by it');
  const run = mutants.steps[measure];
  // The wrapper writes the counts to the job summary and turns cargo-mutants' exit status into the job's.
  assert.match(run.run, /^node scripts\/public\/mutants\.mjs run \S.* -- cargo mutants /);
  for (const argument of [
    '--package "$CRATE"', '--shard "$SHARD/$SHARDS"',
    // nextest applies the default filter in .config/nextest.toml; plain cargo test fails on the filtered tests.
    '--test-tool nextest',
    // A mutant counts as missed only when no test in the repository catches it.
    '--test-workspace=true',
    // In the tree the baseline step passed in. Skipping the baseline leaves no measured time to
    // derive the test timeout from: 120 s is twelve times the 10 s suite.
    '--in-place', '--baseline=skip', '--timeout 120 ',
    // .config/nextest.toml turns fail-fast off; one failing test is enough to call a mutant caught.
    '--cargo-test-arg=--fail-fast',
  ]) assert.ok(run.run.includes(argument), `cargo mutants runs with ${argument}`);
  // The step stops before the job does, so a shard that overruns still uploads what it has.
  assert.ok(run['timeout-minutes'] < mutants['timeout-minutes'], 'the step stops before its job');
  assert.deepEqual(run.env, { CRATE: '${{ matrix.crate }}', SHARD: '${{ matrix.shard }}', SHARDS: '${{ matrix.shards }}' });

  const upload = mutants.steps.find(step => step.uses?.startsWith('actions/upload-artifact@'));
  assert.equal(upload.if, '${{ !cancelled() }}', 'a run that measured nothing still uploads its logs');
  assert.equal(upload.with.name, 'mutants-${{ matrix.crate }}-${{ matrix.shard }}-of-${{ matrix.shards }}');
  assert.equal(upload.with.path, 'mutants.out');

  // One table for the run: a row per crate summed over its shards, written even when a shard failed.
  assert.equal(totals.needs, 'mutants');
  assert.equal(totals.if, '${{ !cancelled() }}');
  const download = totals.steps.findIndex(step => step.uses?.startsWith('actions/download-artifact@'));
  const sum = totals.steps.findIndex(step => step.run?.startsWith('node scripts/public/mutants.mjs totals '));
  assert.ok(download >= 0 && sum > download, 'the shard artifacts are downloaded before they are summed');
  assert.deepEqual(totals.steps[download].with, { pattern: 'mutants-*', path: 'shards' });
  assert.equal(totals.steps[sum].run, 'node scripts/public/mutants.mjs totals shards');
  assert.equal(totals.steps[sum].env.CRATE, planned.env.CRATE, 'the totals expect the shards that were planned');
});
