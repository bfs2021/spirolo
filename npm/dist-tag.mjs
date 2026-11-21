// The npm dist-tag a version publishes under: a prerelease's identifier (`0.1.0-alpha.1` → `alpha`),
// or `latest` for a release. A prerelease never publishes under `latest`; see `takesLatest` for the
// one case where `latest` is pointed at it afterwards.
const semver = /^\d+\.\d+\.\d+(?:-([0-9A-Za-z.-]+))?$/;

export function distTag(version) {
  const match = semver.exec(version);
  if (!match) throw new Error(`Not a semver version: ${version}`);
  if (!match[1]) return 'latest';
  const id = /^[a-z][a-z-]*/i.exec(match[1]);
  if (!id) throw new Error(`Prerelease ${version} has no alphabetic identifier to use as a dist-tag`);
  return id[0].toLowerCase();
}

// Whether `latest` should be pointed at a prerelease once it is published. The npm package page and
// a plain `npm install` both resolve `latest`; while no release exists, leaving it on an older
// prerelease shows and installs a version the documentation no longer names. After the first
// release, `latest` belongs to releases alone and a prerelease never moves it.
export function takesLatest(version, published) {
  const match = semver.exec(version);
  if (!match) throw new Error(`Not a semver version: ${version}`);
  if (!match[1]) return false; // a release already publishes under `latest`
  return !published.some(other => {
    const parsed = semver.exec(other);
    return parsed && !parsed[1];
  });
}
