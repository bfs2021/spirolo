//! The churn reader on input it did not write: `git log --numstat` output in, one churn row
//! per path and diagnostics out. A numstat line is read or reported, and never ends the run.
//!
//! Every log here is written by hand in the shape git documents for `--numstat`
//! (`added<TAB>deleted<TAB>path`, `-` for a binary file, `{old => new}` for a rename), on
//! invented paths.

use spirolo_inputs::{read_numstat, ChurnTable};
use spirolo_model::{Churn, Diagnostic, Severity};

/// One expected churn row.
fn churn(path: &str, added: u64, deleted: u64) -> Churn {
    Churn {
        path: path.to_owned(),
        added,
        deleted,
    }
}

/// The diagnostic the reader gives for line `line` of `churn.txt` when it holds `text`.
fn unreadable(line: usize, text: &str) -> Diagnostic {
    Diagnostic {
        code: "churn_line_unreadable".to_owned(),
        severity: Severity::Warning,
        file: Some("churn.txt".to_owned()),
        span: None,
        message: format!("line {line} is not `added<TAB>deleted<TAB>path`: `{text}`"),
    }
}

/// `git log --numstat` without `--format=`: each commit's header and indented message sit
/// above its numstat lines, and a merge commit has a `Merge:` line and no numstat lines.
const WITH_HEADERS: &str = "\
commit 3f2a9c1d5e7b8a6f4c2d1e0f9a8b7c6d5e4f3a2b
Author: Ada Example <ada@example.com>
Date:   Mon Mar 2 10:15:00 2026 +0100

    Round prices to cents

    Totals drifted by a cent on long baskets.

4\t2\tsrc/price.ts
10\t0\tsrc/price.test.ts

commit 9b8a7c6d5e4f3a2b1c0d9e8f7a6b5c4d3e2f1a0b
Merge: 1a2b3c4 5d6e7f8
Author: Ada Example <ada@example.com>
Date:   Sun Mar 1 18:02:11 2026 +0100

    Merge branch 'basket'

commit 1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0b
Author: Ada Example <ada@example.com>
Date:   Sun Mar 1 17:40:09 2026 +0100

    Add the basket

25\t0\tsrc/basket.ts
";

#[test]
fn numstat_lines_under_commit_headers_become_one_row_per_path_sorted_by_path() {
    let table = read_numstat("churn.txt", WITH_HEADERS.as_bytes());

    assert_eq!(table.diagnostics, vec![]);
    assert_eq!(
        table.churn,
        vec![
            churn("src/basket.ts", 25, 0),
            churn("src/price.test.ts", 10, 0),
            churn("src/price.ts", 4, 2),
        ]
    );
}

#[test]
fn a_log_with_no_lines_or_only_blank_ones_yields_no_rows_and_no_diagnostics() {
    assert_eq!(read_numstat("churn.txt", b""), ChurnTable::default());
    assert_eq!(
        read_numstat("churn.txt", b"\n  \n\n"),
        ChurnTable::default()
    );
}

#[test]
fn a_path_changed_by_several_commits_sums_its_added_and_its_deleted_lines_apart() {
    let table = read_numstat(
        "churn.txt",
        b"5\t1\tsrc/price.ts\n7\t0\tsrc/basket.ts\n\n30\t20\tsrc/price.ts\n0\t4\tsrc/price.ts\n",
    );

    assert_eq!(table.diagnostics, vec![]);
    assert_eq!(
        table.churn,
        vec![
            churn("src/basket.ts", 7, 0),
            churn("src/price.ts", 5 + 30, 1 + 20 + 4),
        ]
    );
}

#[test]
fn a_rename_counts_toward_its_new_path_and_a_binary_row_adds_no_lines() {
    let table = read_numstat(
        "churn.txt",
        b"3\t1\tsrc/{cart => basket}/total.ts\n\
          2\t2\t{lib => src}/tax.ts\n\
          0\t0\tsrc/{money.js => money.ts}\n\
          5\t0\tsrc/{ => money}/round.ts\n\
          1\t1\tsrc/{legacy => }/vat.ts\n\
          0\t0\tdocs/pricing.md => notes/prices.md\n\
          -\t-\tassets/logo.png\n",
    );

    // The new side of each rename, and no doubled slash where a side is empty.
    assert_eq!(table.diagnostics, vec![]);
    assert_eq!(
        table.churn,
        vec![
            churn("assets/logo.png", 0, 0),
            churn("notes/prices.md", 0, 0),
            churn("src/basket/total.ts", 3, 1),
            churn("src/money.ts", 0, 0),
            churn("src/money/round.ts", 5, 0),
            churn("src/tax.ts", 2, 2),
            churn("src/vat.ts", 1, 1),
        ]
    );
}

#[test]
fn a_line_cut_short_is_a_diagnostic_and_the_lines_around_it_still_count() {
    let table = read_numstat(
        "churn.txt",
        b"0\t13\tsrc/basket.ts\n\n5\t5\n2\t0\tsrc/price.ts\n",
    );

    assert_eq!(
        table.churn,
        vec![churn("src/basket.ts", 0, 13), churn("src/price.ts", 2, 0)]
    );
    assert_eq!(table.diagnostics, vec![unreadable(3, "5\t5")]);
}

/// `git log --stat`: the wrong format, a diffstat where a numstat was asked for.
const DIFFSTAT: &str = "\
commit 3f2a9c1d5e7b8a6f4c2d1e0f9a8b7c6d5e4f3a2b
Author: Ada Example <ada@example.com>
Date:   Mon Mar 2 10:15:00 2026 +0100

    Round prices to cents

 src/price.ts | 6 ++++--
 1 file changed, 4 insertions(+), 2 deletions(-)
";

#[test]
fn a_diffstat_is_not_a_numstat_and_each_of_its_lines_is_a_diagnostic() {
    let table = read_numstat("churn.txt", DIFFSTAT.as_bytes());

    // The commit header and message are tolerated; the file line and the summary are not.
    assert_eq!(table.churn, vec![]);
    assert_eq!(
        table.diagnostics,
        vec![
            unreadable(7, " src/price.ts | 6 ++++--"),
            unreadable(8, " 1 file changed, 4 insertions(+), 2 deletions(-)"),
        ]
    );
}

/// A closing brace before the opening one is not git's `prefix{old => new}suffix` rename form.
#[test]
fn a_path_with_its_closing_brace_before_its_opening_brace_is_a_plain_path() {
    let table = read_numstat("churn.txt", b"3\t1\tsrc/a}b{c.ts\n");

    assert_eq!(
        table.churn,
        vec![Churn {
            path: "src/a}b{c.ts".to_owned(),
            added: 3,
            deleted: 1,
        }]
    );
    assert_eq!(table.diagnostics, vec![]);
}
