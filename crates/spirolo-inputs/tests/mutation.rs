//! The mutation report reader on input it did not write: a Stryker `mutation.json` in,
//! `Mutants`, `MutationTest` and `MutationReport` rows and diagnostics out. A report the
//! reader cannot use is a diagnostic, and never ends the run.
//!
//! Every report here is written by hand in the mutation-testing-elements schema Stryker
//! writes (`files[].mutants[]`, `testFiles[].tests[]`, one-based `location`s), on invented
//! files.

use spirolo_inputs::read_mutation_report;
use spirolo_model::{
    Bail, Diagnostic, JoinedCase, MutantStatus, Mutants, MutationReport, MutationTest, Severity,
    Span, TestCase, TestKind,
};

/// The in-tree path the report is read from: Stryker's default file under the package
/// `packages/shop`.
const PATH: &str = "packages/shop/reports/mutation/mutation.json";

/// A `TestCase` of kind test at `file`, `suite_path` and `title`, its span at `start`.
fn test_case(file: &str, suite_path: &[&str], title: &str, start: u32) -> TestCase {
    TestCase {
        file: file.to_owned(),
        span: Span::new(start, start + 1),
        framework: Some("vitest".to_owned()),
        title: Some(title.to_owned()),
        kind: TestKind::Test,
        suite_path: suite_path.iter().map(|s| (*s).to_owned()).collect(),
        assertions: Vec::new(),
        throwing_queries: Vec::new(),
        mocks: Vec::new(),
        skip: None,
        each: None,
        loops: Vec::new(),
        modifiers: Vec::new(),
        tags: Vec::new(),
        body_hash: None,
        empty: false,
        context: Vec::new(),
    }
}

/// A warning about the report at `path`, as the reader words it.
fn warning(code: &str, path: &str, message: &str) -> Diagnostic {
    Diagnostic {
        code: code.to_owned(),
        severity: Severity::Warning,
        file: Some(path.to_owned()),
        span: None,
        message: message.to_owned(),
    }
}

/// The diagnostic for a report with no `testFiles`.
fn no_per_test() -> Diagnostic {
    warning(
        "mutation_report_no_per_test",
        PATH,
        "packages/shop/reports/mutation/mutation.json carries no per-test data; \
         redundant-test cannot use it",
    )
}

/// A list of owned ids.
fn ids(ids: &[&str]) -> Vec<String> {
    ids.iter().map(|id| (*id).to_owned()).collect()
}

/// A report whose one file `src/add.ts` holds `mutants` (the JSON of its `mutants` array,
/// without the brackets), with `head` (top-level members, each ending in a comma) before it
/// and two tests named in `src/add.test.ts`.
fn report_with(head: &str, mutants: &str) -> String {
    format!(
        r#"{{
  "schemaVersion": "1.0",
  {head}
  "files": {{
    "src/add.ts": {{
      "language": "typescript",
      "source": "export const add = (a, b) => a + b\nexport const sub = (a, b) => a - b\n",
      "mutants": [{mutants}]
    }}
  }},
  "testFiles": {{
    "src/add.test.ts": {{
      "tests": [
        {{"id": "t0", "name": "add adds two numbers"}},
        {{"id": "t1", "name": "add handles zero"}}
      ]
    }}
  }}
}}"#
    )
}

/// One mutant's JSON: `id`, `status`, and the ids that covered and killed it.
fn mutant(id: &str, status: &str, covered_by: &[&str], killed_by: &[&str]) -> String {
    format!(
        r#"{{"id": "{id}", "mutatorName": "ArithmeticOperator", "replacement": "a - b",
            "location": {{"start": {{"line": 1, "column": 30}}, "end": {{"line": 1, "column": 35}}}},
            "status": "{status}", "coveredBy": {covered_by:?}, "killedBy": {killed_by:?}}}"#
    )
}

/// A report run with `disableBail` that carries per-test data: `testFiles` name the tests,
/// `coveredBy` and `killedBy` reference them.
const WITH_PER_TEST: &str = r#"{
  "schemaVersion": "1.0",
  "thresholds": {"high": 80, "low": 60},
  "framework": {"name": "StrykerJS", "version": "9.6.1"},
  "config": {"disableBail": true, "coverageAnalysis": "perTest"},
  "files": {
    "src/add.ts": {
      "language": "typescript",
      "source": "export const add = (a, b) => a + b\nexport const sub = (a, b) => a - b\n",
      "mutants": [
        {"id": "0", "mutatorName": "ArithmeticOperator", "replacement": "a - b",
         "location": {"start": {"line": 1, "column": 30}, "end": {"line": 1, "column": 35}},
         "status": "Killed", "coveredBy": ["t1", "t0"], "killedBy": ["t0"]},
        {"id": "1", "mutatorName": "ArithmeticOperator", "replacement": "a + b",
         "location": {"start": {"line": 2, "column": 30}, "end": {"line": 2, "column": 35}},
         "status": "Survived", "coveredBy": ["t1", "t1"], "killedBy": []}
      ]
    }
  },
  "testFiles": {
    "src/add.test.ts": {
      "tests": [
        {"id": "t0", "name": "add adds two numbers"},
        {"id": "t1", "name": "add handles zero"}
      ]
    }
  }
}"#;

#[test]
fn a_report_with_per_test_data_yields_its_mutant_test_and_report_rows_and_no_diagnostic() {
    let tables = read_mutation_report(PATH, WITH_PER_TEST.as_bytes(), &[]);

    // Keys are package-relative in the report and in-tree in the rows; a location is
    // resolved to byte offsets in the file's source; id lists are sorted and deduplicated.
    assert_eq!(
        tables.mutants,
        vec![
            Mutants {
                file: "packages/shop/src/add.ts".to_owned(),
                span: Span {
                    start: 29,
                    end: 34,
                    line: 1,
                    column: 30,
                },
                id: "0".to_owned(),
                mutator: Some("ArithmeticOperator".to_owned()),
                status: MutantStatus::Killed,
                killed_by: ids(&["t0"]),
                covered_by: ids(&["t0", "t1"]),
                report: PATH.to_owned(),
            },
            Mutants {
                file: "packages/shop/src/add.ts".to_owned(),
                span: Span {
                    start: 64,
                    end: 69,
                    line: 2,
                    column: 30,
                },
                id: "1".to_owned(),
                mutator: Some("ArithmeticOperator".to_owned()),
                status: MutantStatus::Survived,
                killed_by: ids(&[]),
                covered_by: ids(&["t1"]),
                report: PATH.to_owned(),
            },
        ]
    );
    // No model tests to join against, so every named test is unjoined.
    assert_eq!(
        tables.tests,
        vec![
            MutationTest {
                report: PATH.to_owned(),
                id: "t0".to_owned(),
                file: "packages/shop/src/add.test.ts".to_owned(),
                name: "add adds two numbers".to_owned(),
                joined: None,
            },
            MutationTest {
                report: PATH.to_owned(),
                id: "t1".to_owned(),
                file: "packages/shop/src/add.test.ts".to_owned(),
                name: "add handles zero".to_owned(),
                joined: None,
            },
        ]
    );
    assert_eq!(
        tables.reports,
        vec![MutationReport {
            path: PATH.to_owned(),
            schema_version: Some("1.0".to_owned()),
            tool: Some("StrykerJS".to_owned()),
            tool_version: Some("9.6.1".to_owned()),
            per_test_data: true,
            bail: Bail::Disabled,
        }]
    );
    assert_eq!(tables.diagnostics, vec![]);
}

#[test]
fn a_report_with_no_files_yields_its_report_row_and_says_it_has_no_per_test_data() {
    let tables = read_mutation_report(PATH, br#"{"files": {}}"#, &[]);

    assert_eq!(tables.mutants, vec![]);
    assert_eq!(tables.tests, vec![]);
    assert_eq!(
        tables.reports,
        vec![MutationReport {
            path: PATH.to_owned(),
            schema_version: None,
            tool: None,
            tool_version: None,
            per_test_data: false,
            bail: Bail::Unknown,
        }]
    );
    assert_eq!(tables.diagnostics, vec![no_per_test()]);
}

#[test]
fn a_report_that_names_test_files_but_no_test_has_no_per_test_data() {
    let report = r#"{"files": {}, "testFiles": {"src/add.test.ts": {"tests": []}}}"#;
    let tables = read_mutation_report(PATH, report.as_bytes(), &[]);

    assert_eq!(tables.tests, vec![]);
    assert!(!tables.reports[0].per_test_data);
    assert_eq!(tables.diagnostics, vec![no_per_test()]);
}

/// The rows and the one diagnostic of a report whose bytes the reader cannot use.
fn assert_unreadable(bytes: &[u8]) {
    let tables = read_mutation_report(PATH, bytes, &[]);

    assert_eq!(tables.mutants, vec![]);
    assert_eq!(tables.tests, vec![]);
    assert_eq!(tables.reports, vec![]);
    assert_eq!(tables.diagnostics.len(), 1);
    let diagnostic = &tables.diagnostics[0];
    assert_eq!(diagnostic.code, "mutation_report_unreadable");
    assert_eq!(diagnostic.severity, Severity::Error);
    assert_eq!(diagnostic.file.as_deref(), Some(PATH));
    assert_eq!(diagnostic.span, None);
    assert!(
        diagnostic.message.starts_with(
            "packages/shop/reports/mutation/mutation.json is not a readable mutation report: "
        ),
        "{}",
        diagnostic.message
    );
}

#[test]
fn bytes_that_are_not_json_are_one_diagnostic_and_no_rows() {
    assert_unreadable(b"");
    assert_unreadable(b"{ not json");
    // A report cut off mid-write.
    assert_unreadable(&WITH_PER_TEST.as_bytes()[..WITH_PER_TEST.len() / 2]);
}

/// One mutant record without its `location` is not skipped on its own: the whole report
/// is unreadable, the well-formed mutant beside it included.
/// Current behaviour, pinned so a change is a decision and not an endorsement; see #27.
#[test]
fn a_mutant_record_without_its_location_makes_the_whole_report_unreadable() {
    let report = report_with(
        "",
        &format!(
            r#"{}, {{"id": "1", "mutatorName": "ArithmeticOperator", "status": "Survived"}}"#,
            mutant("0", "Killed", &["t0"], &["t0"])
        ),
    );

    assert_unreadable(report.as_bytes());
}

#[test]
fn each_status_the_schema_names_is_read_and_one_it_does_not_name_reads_as_pending() {
    let statuses = [
        ("Killed", MutantStatus::Killed),
        ("Survived", MutantStatus::Survived),
        ("NoCoverage", MutantStatus::NoCoverage),
        ("Timeout", MutantStatus::Timeout),
        ("RuntimeError", MutantStatus::RuntimeError),
        ("CompileError", MutantStatus::CompileError),
        ("Ignored", MutantStatus::Ignored),
        ("Pending", MutantStatus::Pending),
        ("Quarantined", MutantStatus::Pending),
    ];
    for (written, read) in statuses {
        let report = report_with("", &mutant("0", written, &[], &[]));
        let tables = read_mutation_report(PATH, report.as_bytes(), &[]);

        assert_eq!(tables.mutants[0].status, read, "{written}");
    }
}

#[test]
fn a_mutant_whose_file_has_no_source_keeps_its_line_and_column_with_zero_offsets() {
    let report = r#"{"files": {"src/add.ts": {"mutants": [
        {"id": "0", "status": "NoCoverage",
         "location": {"start": {"line": 2, "column": 30}, "end": {"line": 2, "column": 35}}}
    ]}}}"#;
    let tables = read_mutation_report(PATH, report.as_bytes(), &[]);

    assert_eq!(
        tables.mutants,
        vec![Mutants {
            file: "packages/shop/src/add.ts".to_owned(),
            span: Span {
                start: 0,
                end: 0,
                line: 2,
                column: 30,
            },
            id: "0".to_owned(),
            mutator: None,
            status: MutantStatus::NoCoverage,
            killed_by: vec![],
            covered_by: vec![],
            report: PATH.to_owned(),
        }]
    );
}

/// A column past its line's end is clamped to the line's length with its newline, so it
/// lands on the first byte of the next line; a line past the last is clamped to the
/// source's length; `"column": 0`, below the schema's minimum of 1, reads as column 1; and
/// an end before its start is kept as an inverted span.
/// Current behaviour, pinned so a change is a decision and not an endorsement; see #27.
#[test]
fn a_location_outside_the_source_is_clamped_to_its_line_or_to_the_source() {
    // Two lines of four bytes each, the newline included.
    let report = r#"{"files": {"src/add.ts": {"source": "abc\ndef\n", "mutants": [
        {"id": "past the line", "status": "Survived",
         "location": {"start": {"line": 2, "column": 2}, "end": {"line": 1, "column": 99}}},
        {"id": "past the source", "status": "Survived",
         "location": {"start": {"line": 1, "column": 0}, "end": {"line": 7, "column": 1}}}
    ]}}}"#;
    let tables = read_mutation_report(PATH, report.as_bytes(), &[]);

    let spans: Vec<(&str, Span)> = tables
        .mutants
        .iter()
        .map(|mutant| (mutant.id.as_str(), mutant.span))
        .collect();
    assert_eq!(
        spans,
        vec![
            (
                "past the line",
                Span {
                    start: 5,
                    end: 4,
                    line: 2,
                    column: 2,
                }
            ),
            (
                "past the source",
                Span {
                    start: 0,
                    end: 8,
                    line: 1,
                    column: 0,
                }
            ),
        ]
    );
}

/// The report's bail state and the codes of its diagnostics.
fn bail_and_codes(report: &str) -> (Bail, Vec<String>) {
    let tables = read_mutation_report(PATH, report.as_bytes(), &[]);
    (
        tables.reports[0].bail,
        tables
            .diagnostics
            .into_iter()
            .map(|diagnostic| diagnostic.code)
            .collect(),
    )
}

#[test]
fn bail_declared_by_the_config_is_a_diagnostic() {
    let report = report_with(
        r#""config": {"disableBail": false},"#,
        &mutant("0", "Killed", &["t0", "t1"], &["t0", "t1"]),
    );
    let tables = read_mutation_report(PATH, report.as_bytes(), &[]);

    assert_eq!(tables.reports[0].bail, Bail::Declared);
    assert_eq!(
        tables.diagnostics,
        vec![warning(
            "mutation_report_bailed",
            PATH,
            "packages/shop/reports/mutation/mutation.json was produced with bail; re-run with \
             disableBail so every killing test is recorded",
        )]
    );
}

#[test]
fn bail_disabled_by_the_config_is_believed_whatever_the_kill_sets_look_like() {
    let report = report_with(
        r#""config": {"disableBail": true},"#,
        &mutant("0", "Killed", &["t0", "t1"], &["t0"]),
    );

    assert_eq!(bail_and_codes(&report), (Bail::Disabled, vec![]));
}

/// With no config, bail is inferred from the shape truncation leaves: every killed mutant
/// names exactly one killer while a mutant is covered by more than one test. A mutant that
/// was not killed names no killer and does not count against it.
#[test]
fn bail_inferred_from_the_kill_sets_is_a_diagnostic() {
    let mutants = [
        mutant("0", "Killed", &["t0", "t1"], &["t0"]),
        mutant("1", "Killed", &["t1"], &["t1"]),
        mutant("2", "Survived", &["t0"], &[]),
    ]
    .join(",");
    let tables = read_mutation_report(PATH, report_with("", &mutants).as_bytes(), &[]);

    assert_eq!(tables.reports[0].bail, Bail::Inferred);
    assert_eq!(
        tables.diagnostics,
        vec![warning(
            "mutation_report_bailed",
            PATH,
            "packages/shop/reports/mutation/mutation.json appears to have been produced with \
             bail; re-run with disableBail so every killing test is recorded",
        )]
    );
}

#[test]
fn bail_stays_unknown_and_unreported_when_the_kill_sets_do_not_show_it() {
    let no_mutant_killed = mutant("0", "Survived", &["t0", "t1"], &[]);
    let a_mutant_with_two_killers = [
        mutant("0", "Killed", &["t0", "t1"], &["t0"]),
        mutant("1", "Killed", &["t0", "t1"], &["t0", "t1"]),
    ]
    .join(",");
    let no_mutant_covered_twice = [
        mutant("0", "Killed", &["t0"], &["t0"]),
        mutant("1", "Killed", &["t1"], &["t1"]),
    ]
    .join(",");

    for mutants in [
        no_mutant_killed,
        a_mutant_with_two_killers,
        no_mutant_covered_twice,
    ] {
        assert_eq!(
            bail_and_codes(&report_with("", &mutants)),
            (Bail::Unknown, vec![]),
            "{mutants}"
        );
    }
}

/// Stryker names a test by its suite titles and title joined with single spaces, so the
/// join compares words: where a title ends and how it is spaced do not matter.
#[test]
fn a_named_test_joins_the_model_test_in_its_file_with_the_same_words() {
    let file = "packages/shop/src/add.test.ts";
    let model = vec![
        // Same words, in another file.
        test_case(
            "packages/shop/src/sub.test.ts",
            &["add"],
            "handles zero",
            10,
        ),
        // Same words, but a suite, not a test.
        TestCase {
            kind: TestKind::Suite,
            ..test_case(file, &["add"], "handles zero", 20)
        },
        // A title that is not a literal names nothing.
        TestCase {
            title: None,
            ..test_case(file, &["add", "handles", "zero"], "", 30)
        },
        test_case(file, &["add  adds"], " two\tnumbers", 40),
    ];
    let tables = read_mutation_report(PATH, WITH_PER_TEST.as_bytes(), &model);

    let joined: Vec<(&str, Option<&JoinedCase>)> = tables
        .tests
        .iter()
        .map(|test| (test.name.as_str(), test.joined.as_ref()))
        .collect();
    assert_eq!(
        joined,
        vec![
            (
                "add adds two numbers",
                Some(&JoinedCase {
                    file: file.to_owned(),
                    span: Span::new(40, 41),
                })
            ),
            ("add handles zero", None),
        ]
    );
}

#[test]
fn a_test_whose_words_only_start_like_a_model_tests_stays_unjoined() {
    let file = "packages/shop/src/add.test.ts";
    let model = vec![
        test_case(file, &["add"], "adds two", 10),
        test_case(file, &["add"], "adds two numbers twice", 20),
    ];
    let tables = read_mutation_report(PATH, WITH_PER_TEST.as_bytes(), &model);

    assert_eq!(tables.tests[0].name, "add adds two numbers");
    assert_eq!(tables.tests[0].joined, None);
}

/// The file of the first mutant and of the first test of [`WITH_PER_TEST`] read from
/// `path`, with `config` as the report's `config` member.
fn files_read_from(path: &str, config: &str) -> (String, String) {
    let report = WITH_PER_TEST.replacen(
        r#"{"disableBail": true, "coverageAnalysis": "perTest"}"#,
        config,
        1,
    );
    let tables = read_mutation_report(path, report.as_bytes(), &[]);
    (tables.mutants[0].file.clone(), tables.tests[0].file.clone())
}

#[test]
fn a_report_at_the_tree_root_keeps_its_keys_as_they_are() {
    assert_eq!(
        files_read_from("reports/mutation/mutation.json", "{}"),
        ("src/add.ts".to_owned(), "src/add.test.ts".to_owned())
    );
}

#[test]
fn a_report_written_where_the_config_says_resolves_against_the_directory_above_that_name() {
    let config = r#"{"jsonReporter": {"fileName": "./out/stryker.json"}}"#;

    assert_eq!(
        files_read_from("packages/shop/out/stryker.json", config),
        (
            "packages/shop/src/add.ts".to_owned(),
            "packages/shop/src/add.test.ts".to_owned()
        )
    );
}

/// A report that is not where Stryker would have written it names no package: its keys
/// are kept as they are instead of being put under a guessed directory.
#[test]
fn a_report_that_is_not_at_its_expected_file_name_keeps_its_keys_as_they_are() {
    let unprefixed = ("src/add.ts".to_owned(), "src/add.test.ts".to_owned());

    assert_eq!(
        files_read_from("packages/shop/saved/last-night.json", "{}"),
        unprefixed
    );
    // The expected name, but not after a directory boundary.
    assert_eq!(
        files_read_from("packages/shop/old-reports/mutation/mutation.json", "{}"),
        unprefixed
    );
}
