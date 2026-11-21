//! The churn reader on input it did not write: a numstat line is read or reported, and never
//! ends the run.

use spirolo_inputs::read_numstat;
use spirolo_model::Churn;

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
