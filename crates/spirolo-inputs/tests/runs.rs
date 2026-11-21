//! The run report readers on input they did not write: a vitest JSON or `JUnit` XML report
//! in, `Run` rows and diagnostics out. A report or a record a reader cannot use is a
//! diagnostic, and never ends the run.
//!
//! Every report here is written by hand, on invented files, in the shape its writer
//! documents: vitest's Jest-compatible JSON (`testResults[].assertionResults[]`), vitest's
//! `junit` reporter (`testsuite`/`testcase`), and Maven Surefire's retry children.

use spirolo_inputs::{read_junit_xml, read_vitest_json, RunContext, RunTables};
use spirolo_model::{Diagnostic, JoinedCase, Run, RunOutcome, Severity, Span, TestCase, TestKind};

/// The project root the reports' absolute paths lie under.
const ROOT: &str = "/work/shop";

/// 2026-10-01T10:00:00Z in milliseconds since the Unix epoch.
const STARTED: u64 = 1_790_848_800_000;

/// What the binary knows about the run when it reads `report`.
fn context(report: &str) -> RunContext {
    RunContext {
        runner_home: Some("vitest@shop".to_owned()),
        report: report.to_owned(),
        root: ROOT.to_owned(),
        run_id: "run-1".to_owned(),
        attempt: 2,
        condition: Some("idle".to_owned()),
        tree_hash: Some("abc123".to_owned()),
    }
}

/// A list of owned title parts.
fn path(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|part| (*part).to_owned()).collect()
}

/// The row [`context`] gives a test of `file` at `title_path`, started at [`STARTED`] and
/// joined to nothing.
fn run(
    report: &str,
    file: &str,
    title_path: &[&str],
    outcome: RunOutcome,
    duration_ms: Option<u64>,
    retries: Option<u32>,
) -> Run {
    Run {
        file: file.to_owned(),
        title: title_path.join(" > "),
        run_id: "run-1".to_owned(),
        attempt: 2,
        title_path: path(title_path),
        outcome,
        duration_ms,
        retries,
        tree_hash: Some("abc123".to_owned()),
        started_at_ms: Some(STARTED),
        condition: Some("idle".to_owned()),
        report: report.to_owned(),
        runner_home: Some("vitest@shop".to_owned()),
        joined: None,
    }
}

/// A `TestCase` of kind test at `file`, `suite_path` and `title`, its span at `start`.
fn test_case(file: &str, suite_path: &[&str], title: &str, start: u32) -> TestCase {
    TestCase {
        file: file.to_owned(),
        span: Span::new(start, start + 1),
        framework: Some("vitest".to_owned()),
        title: Some(title.to_owned()),
        kind: TestKind::Test,
        suite_path: path(suite_path),
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

/// A warning about the report at `report`, as the readers word it.
fn warning(code: &str, report: &str, message: &str) -> Diagnostic {
    Diagnostic {
        code: code.to_owned(),
        severity: Severity::Warning,
        file: Some(report.to_owned()),
        span: None,
        message: message.to_owned(),
    }
}

/// Each row as (file, title, outcome, retries).
fn outcomes(runs: &[Run]) -> Vec<(&str, &str, RunOutcome, Option<u32>)> {
    runs.iter()
        .map(|run| {
            (
                run.file.as_str(),
                run.title.as_str(),
                run.outcome,
                run.retries,
            )
        })
        .collect()
}

/// Asserts `tables` is what a reader gives for bytes it cannot parse as `format`.
fn assert_unreadable(tables: &RunTables, report: &str, format: &str) {
    assert_eq!(tables.runs, vec![]);
    assert_eq!(tables.diagnostics.len(), 1);
    let diagnostic = &tables.diagnostics[0];
    assert_eq!(diagnostic.code, "run_report_unreadable");
    assert_eq!(diagnostic.severity, Severity::Error);
    assert_eq!(diagnostic.file.as_deref(), Some(report));
    assert_eq!(diagnostic.span, None);
    let opening = format!("{report} is not a readable {format} report: ");
    assert!(
        diagnostic.message.starts_with(&opening),
        "{}",
        diagnostic.message
    );
}

// ------------------------------------------------------------ vitest JSON

/// `vitest run --reporter=json` on two files: a pass, a fail, a skip, a todo, a top-level
/// test, and a test that passed on its second attempt (vitest keeps the first attempt's
/// message on it). The files are named absolutely, and not in the order the rows sort in.
const VITEST_JSON: &str = r#"{
  "numTotalTestSuites": 5,
  "numPassedTestSuites": 4,
  "numFailedTestSuites": 1,
  "numPendingTestSuites": 0,
  "numTotalTests": 6,
  "numPassedTests": 3,
  "numFailedTests": 1,
  "numPendingTests": 1,
  "numTodoTests": 1,
  "startTime": 1790848800000,
  "success": false,
  "testResults": [
    {
      "assertionResults": [
        {
          "ancestorTitles": ["price", "round"],
          "fullName": "price round rounds half up",
          "status": "passed",
          "title": "rounds half up",
          "duration": 1.6,
          "failureMessages": [],
          "meta": {}
        },
        {
          "ancestorTitles": ["price", "round"],
          "fullName": "price round keeps two decimals",
          "status": "failed",
          "title": "keeps two decimals",
          "duration": 3.4,
          "failureMessages": ["AssertionError: expected 1 to be 1.01 // Object.is equality"],
          "location": {"line": 9, "column": 22},
          "meta": {}
        },
        {
          "ancestorTitles": ["price"],
          "fullName": "price is parked",
          "status": "skipped",
          "title": "is parked",
          "failureMessages": [],
          "meta": {}
        },
        {
          "ancestorTitles": ["price"],
          "fullName": "price handles currencies",
          "status": "todo",
          "title": "handles currencies",
          "failureMessages": [],
          "meta": {}
        },
        {
          "ancestorTitles": [],
          "fullName": "sits at the top level",
          "status": "passed",
          "title": "sits at the top level",
          "duration": 0.4,
          "failureMessages": [],
          "meta": {}
        }
      ],
      "startTime": 1790848800120,
      "endTime": 1790848800126,
      "status": "failed",
      "message": "",
      "name": "/work/shop/src/price.test.ts"
    },
    {
      "assertionResults": [
        {
          "ancestorTitles": ["basket"],
          "fullName": "basket settles on the second attempt",
          "status": "passed",
          "title": "settles on the second attempt",
          "duration": 12,
          "failureMessages": ["Error: not settled yet"],
          "meta": {}
        }
      ],
      "startTime": 1790848800120,
      "endTime": 1790848800132,
      "status": "passed",
      "message": "",
      "name": "/work/shop/src/basket.test.ts"
    }
  ]
}"#;

/// Every field of every row: files made root-relative, durations rounded to whole
/// milliseconds, retries counted from the messages vitest keeps per failed attempt, the
/// run's start from the report, the context on every row, and the rows sorted.
#[test]
fn a_vitest_json_report_yields_one_row_per_result() {
    let report = "reports/run-1.json";
    let tables = read_vitest_json(&context(report), VITEST_JSON.as_bytes(), &[]);

    let price = "src/price.test.ts";
    assert_eq!(tables.diagnostics, vec![]);
    assert_eq!(
        tables.runs,
        vec![
            run(
                report,
                "src/basket.test.ts",
                &["basket", "settles on the second attempt"],
                RunOutcome::PassedOnRetry,
                Some(12),
                Some(1),
            ),
            run(
                report,
                price,
                &["price", "handles currencies"],
                RunOutcome::Skipped,
                None,
                None,
            ),
            run(
                report,
                price,
                &["price", "is parked"],
                RunOutcome::Skipped,
                None,
                None,
            ),
            run(
                report,
                price,
                &["price", "round", "keeps two decimals"],
                RunOutcome::Failed,
                Some(3),
                Some(0),
            ),
            run(
                report,
                price,
                &["price", "round", "rounds half up"],
                RunOutcome::Passed,
                Some(2),
                Some(0),
            ),
            run(
                report,
                price,
                &["sits at the top level"],
                RunOutcome::Passed,
                Some(0),
                Some(0),
            ),
        ]
    );
}

#[test]
fn a_vitest_json_report_with_no_results_yields_no_rows_and_no_diagnostics() {
    for report in [r#"{"startTime": 1790848800000, "testResults": []}"#, "{}"] {
        let tables = read_vitest_json(&context("report.json"), report.as_bytes(), &[]);

        assert_eq!(tables.runs, vec![], "{report}");
        assert_eq!(tables.diagnostics, vec![], "{report}");
    }
}

#[test]
fn vitest_json_that_does_not_parse_is_one_diagnostic_and_no_rows() {
    let report = "reports/run-1.json";
    // A report cut off mid-write, then no bytes at all.
    let truncated = &VITEST_JSON.as_bytes()[..VITEST_JSON.len() / 2];
    for bytes in [truncated, b"".as_slice()] {
        let tables = read_vitest_json(&context(report), bytes, &[]);

        assert_unreadable(&tables, report, "vitest JSON");
    }
}

/// One result without its `status` is not skipped on its own: the whole report is
/// unreadable, the well-formed results beside it included.
/// Current behaviour, pinned so a change is a decision and not an endorsement; see #27.
#[test]
fn a_vitest_result_without_its_status_makes_the_whole_report_unreadable() {
    let report = VITEST_JSON.replacen(r#""status": "todo","#, "", 1);
    let tables = read_vitest_json(&context("report.json"), report.as_bytes(), &[]);

    assert_unreadable(&tables, "report.json", "vitest JSON");
}

/// A report that leaves out what the reader can do without: no start time, a result with
/// no duration, ancestors or messages.
#[test]
fn a_vitest_result_with_only_its_title_and_status_is_still_a_row() {
    let report = r#"{"testResults": [
        {"name": "src/price.test.ts", "assertionResults": [{"title": "rounds", "status": "passed"}]}
    ]}"#;
    let tables = read_vitest_json(&context("report.json"), report.as_bytes(), &[]);

    assert_eq!(tables.diagnostics, vec![]);
    assert_eq!(
        tables.runs,
        vec![Run {
            started_at_ms: None,
            ..run(
                "report.json",
                "src/price.test.ts",
                &["rounds"],
                RunOutcome::Passed,
                None,
                Some(0),
            )
        }]
    );
}

/// A report whose one file `src/price.test.ts` holds `results` (the JSON of its
/// `assertionResults` array, without the brackets).
fn vitest_file(results: &str) -> String {
    format!(
        r#"{{"startTime": 1790848800000.4, "testResults": [
            {{"name": "/work/shop/src/price.test.ts", "status": "failed", "message": "",
              "assertionResults": [{results}]}}
        ]}}"#
    )
}

/// One result's JSON: its title, status, duration and the messages of its failed attempts.
fn vitest_result(title: &str, status: &str, duration: &str, messages: &[&str]) -> String {
    format!(
        r#"{{"ancestorTitles": [], "title": "{title}", "status": "{status}",
            "duration": {duration}, "failureMessages": {messages:?}}}"#
    )
}

/// Vitest keeps one message per failed attempt: a pass after `n` messages took `n`
/// retries, a fail after `n` messages took `n - 1`, and a test that never ran took none.
#[test]
fn vitest_retries_are_counted_from_the_messages_of_the_failed_attempts() {
    let results = [
        vitest_result("a passes", "passed", "1", &[]),
        vitest_result("b passes late", "passed", "1", &["first", "second"]),
        vitest_result("c fails", "failed", "1", &["first", "second", "third"]),
        vitest_result("d fails silently", "failed", "1", &[]),
        vitest_result("e is pending", "pending", "0", &[]),
    ]
    .join(",");
    let tables = read_vitest_json(
        &context("report.json"),
        vitest_file(&results).as_bytes(),
        &[],
    );

    let file = "src/price.test.ts";
    assert_eq!(
        outcomes(&tables.runs),
        vec![
            (file, "a passes", RunOutcome::Passed, Some(0)),
            (file, "b passes late", RunOutcome::PassedOnRetry, Some(2)),
            (file, "c fails", RunOutcome::Failed, Some(2)),
            (file, "d fails silently", RunOutcome::Failed, Some(0)),
            (file, "e is pending", RunOutcome::Skipped, None),
        ]
    );
}

#[test]
fn vitest_milliseconds_are_rounded_to_whole_ones_and_a_negative_one_reads_as_zero() {
    let results = [
        vitest_result("a", "passed", "2.5", &[]),
        vitest_result("b", "passed", "2.49", &[]),
        vitest_result("c", "passed", "-3", &[]),
        vitest_result("d", "passed", "0", &[]),
        vitest_result("e", "passed", "null", &[]),
    ]
    .join(",");
    let tables = read_vitest_json(
        &context("report.json"),
        vitest_file(&results).as_bytes(),
        &[],
    );

    let durations: Vec<(&str, Option<u64>, Option<u64>)> = tables
        .runs
        .iter()
        .map(|run| (run.title.as_str(), run.duration_ms, run.started_at_ms))
        .collect();
    assert_eq!(
        durations,
        vec![
            ("a", Some(3), Some(STARTED)),
            ("b", Some(2), Some(STARTED)),
            ("c", Some(0), Some(STARTED)),
            ("d", Some(0), Some(STARTED)),
            ("e", None, Some(STARTED)),
        ]
    );
}

/// A relative name is taken as root-relative already. An absolute one must lie under the
/// root, a directory whose name only starts like the root's does not, and what is outside
/// is one diagnostic per file, its rows not attributed to a guessed path.
#[test]
fn a_vitest_file_outside_the_root_is_a_diagnostic_and_its_results_are_not_recorded() {
    let passed = r#"{"title": "rounds", "status": "passed"}"#;
    let report = format!(
        r#"{{"testResults": [
            {{"name": "src/relative.test.ts", "assertionResults": [{passed}]}},
            {{"name": "/work/shop/src/inside.test.ts", "assertionResults": [{passed}]}},
            {{"name": "/work/shop-old/src/sibling.test.ts", "assertionResults": [{passed}, {passed}]}},
            {{"name": "/elsewhere/src/far.test.ts", "assertionResults": []}}
        ]}}"#
    );
    // The root as a user may write it, with its trailing slash.
    let context = RunContext {
        root: "/work/shop/".to_owned(),
        ..context("reports/run-1.json")
    };
    let tables = read_vitest_json(&context, report.as_bytes(), &[]);

    let files: Vec<&str> = tables.runs.iter().map(|run| run.file.as_str()).collect();
    assert_eq!(files, vec!["src/inside.test.ts", "src/relative.test.ts"]);
    assert_eq!(
        tables.diagnostics,
        vec![
            warning(
                "run_report_path_outside_root",
                "reports/run-1.json",
                "reports/run-1.json names /work/shop-old/src/sibling.test.ts, which is not \
                 under /work/shop/; its 2 results are not recorded",
            ),
            warning(
                "run_report_path_outside_root",
                "reports/run-1.json",
                "reports/run-1.json names /elsewhere/src/far.test.ts, which is not under \
                 /work/shop/; its 0 results are not recorded",
            ),
        ]
    );
}

/// Three files of one run. `hook.test.ts` failed in a `beforeAll`: vitest marks the file
/// failed with a message of its own and reports its test as skipped. `load.test.ts` failed
/// at import and has no results. `steady.test.ts` failed only through a test, so its
/// message is empty and its parked test really was skipped.
const FILE_LEVEL_JSON: &str = r#"{
  "startTime": 1790848800000,
  "success": false,
  "testResults": [
    {
      "assertionResults": [
        {"ancestorTitles": [], "fullName": "reads the seeded rows", "status": "skipped",
         "title": "reads the seeded rows", "failureMessages": [], "meta": {}}
      ],
      "status": "failed",
      "message": "seed failed",
      "name": "/work/shop/src/hook.test.ts"
    },
    {
      "assertionResults": [],
      "status": "failed",
      "message": "Failed to load url ./missing (resolved id: ./missing) in /work/shop/src/load.test.ts. Does the file exist?",
      "name": "/work/shop/src/load.test.ts"
    },
    {
      "assertionResults": [
        {"ancestorTitles": [], "fullName": "is parked", "status": "skipped",
         "title": "is parked", "failureMessages": [], "meta": {}},
        {"ancestorTitles": [], "fullName": "is red", "status": "failed", "title": "is red",
         "duration": 2, "failureMessages": ["AssertionError: expected 1 to be 2"], "meta": {}}
      ],
      "status": "failed",
      "message": "",
      "name": "/work/shop/src/steady.test.ts"
    }
  ]
}"#;

#[test]
fn a_vitest_file_that_failed_outside_its_tests_gets_a_file_level_row() {
    let report = "report.json";
    let tables = read_vitest_json(&context(report), FILE_LEVEL_JSON.as_bytes(), &[]);

    assert_eq!(tables.diagnostics, vec![]);
    assert_eq!(
        tables.runs,
        vec![
            // The file-level row: an empty title path, failed.
            run(
                report,
                "src/hook.test.ts",
                &[],
                RunOutcome::Failed,
                None,
                None
            ),
            // Reported as skipped, but it never got to run.
            run(
                report,
                "src/hook.test.ts",
                &["reads the seeded rows"],
                RunOutcome::Failed,
                None,
                None,
            ),
            run(
                report,
                "src/load.test.ts",
                &[],
                RunOutcome::Failed,
                None,
                None
            ),
            run(
                report,
                "src/steady.test.ts",
                &["is parked"],
                RunOutcome::Skipped,
                None,
                None,
            ),
            run(
                report,
                "src/steady.test.ts",
                &["is red"],
                RunOutcome::Failed,
                Some(2),
                Some(0),
            ),
        ]
    );
}

/// A file with a message that vitest did not mark failed is not a file-level failure. No
/// known writer emits this: vitest gives a message only to a file it marks failed.
#[test]
fn a_vitest_file_with_a_message_that_did_not_fail_gets_no_file_level_row() {
    let report = r#"{"testResults": [
        {"name": "src/price.test.ts", "status": "passed", "message": "seed failed",
         "assertionResults": [{"title": "is parked", "status": "skipped"}]}
    ]}"#;
    let tables = read_vitest_json(&context("report.json"), report.as_bytes(), &[]);

    assert_eq!(
        outcomes(&tables.runs),
        vec![("src/price.test.ts", "is parked", RunOutcome::Skipped, None)]
    );
}

/// The model's tests the join is tried against, sorted by file as the model keeps them.
fn model() -> Vec<TestCase> {
    let price = "src/price.test.ts";
    vec![
        test_case("src/basket.test.ts", &[], "sits at the top level", 5),
        test_case(price, &["price", "round"], "rounds half up", 10),
        // Same title, another suite path: not the same test.
        test_case(price, &["price"], "keeps two decimals", 20),
        // Same title path, but a suite and a test with no literal title: not candidates.
        TestCase {
            kind: TestKind::Suite,
            ..test_case(price, &["price"], "is parked", 30)
        },
        TestCase {
            title: None,
            ..test_case(price, &["price"], "handles currencies", 40)
        },
        test_case(price, &[], "sits at the top level", 50),
        // Same title path, a later file: not the same test.
        test_case("src/tax.test.ts", &["price"], "is parked", 60),
    ]
}

/// Each row's title and what it joined.
fn joins(runs: &[Run]) -> Vec<(&str, Option<&JoinedCase>)> {
    runs.iter()
        .map(|run| (run.title.as_str(), run.joined.as_ref()))
        .collect()
}

/// The join a row of `src/price.test.ts` makes with the case at `start`.
fn price_case_at(start: u32) -> JoinedCase {
    JoinedCase {
        file: "src/price.test.ts".to_owned(),
        span: Span::new(start, start + 1),
    }
}

#[test]
fn a_vitest_row_joins_the_test_with_its_file_and_title_path_and_an_unmatched_row_is_kept() {
    let tables = read_vitest_json(&context("report.json"), VITEST_JSON.as_bytes(), &model());

    assert_eq!(
        joins(&tables.runs),
        vec![
            ("basket > settles on the second attempt", None),
            ("price > handles currencies", None),
            ("price > is parked", None),
            ("price > round > keeps two decimals", None),
            ("price > round > rounds half up", Some(&price_case_at(10))),
            ("sits at the top level", Some(&price_case_at(50))),
        ]
    );
}

// -------------------------------------------------------------- JUnit XML

/// The run of [`VITEST_JSON`] as vitest's `junit` reporter writes it: `classname` is the
/// root-relative file, `name` the title path joined by `" > "`, `time` is in seconds, and
/// a skipped or todo test has a `skipped` child. Vitest's `JUnit` keeps no trace of a failed
/// attempt that a retry passed, so that test is a plain pass here.
const VITEST_JUNIT: &str = r#"<?xml version="1.0" encoding="UTF-8" ?>
<testsuites name="vitest tests" tests="6" failures="1" errors="0" time="0.018">
    <testsuite name="src/price.test.ts" timestamp="2026-10-01T10:00:05.120Z" hostname="builder" tests="5" failures="1" errors="0" skipped="2" time="0.006">
        <testcase classname="src/price.test.ts" name="price &gt; round &gt; rounds half up" time="0.0016">
        </testcase>
        <testcase classname="src/price.test.ts" name="price &gt; round &gt; keeps two decimals" time="0.0034">
            <failure message="expected 1 to be 1.01 // Object.is equality" type="AssertionError">
AssertionError: expected 1 to be 1.01 // Object.is equality
 ❯ src/price.test.ts:9:22
            </failure>
        </testcase>
        <testcase classname="src/price.test.ts" name="price &gt; is parked" time="0">
            <skipped/>
        </testcase>
        <testcase classname="src/price.test.ts" name="price &gt; handles currencies" time="0">
            <skipped/>
        </testcase>
        <testcase classname="src/price.test.ts" name="sits at the top level" time="0.0004">
        </testcase>
    </testsuite>
    <testsuite name="src/basket.test.ts" timestamp="2026-10-01T10:00:00.000Z" hostname="builder" tests="1" failures="0" errors="0" skipped="0" time="0.012">
        <testcase classname="src/basket.test.ts" name="basket &gt; settles on the second attempt" time="0.012">
        </testcase>
    </testsuite>
</testsuites>
"#;

/// Every field of every row: `time` in seconds to whole milliseconds, one retry less than
/// the `failure` children, the run's start from the earliest `testsuite` timestamp, the
/// context on every row, and the rows sorted.
#[test]
fn a_junit_report_yields_one_row_per_testcase() {
    let report = "reports/run-1.xml";
    let tables = read_junit_xml(&context(report), VITEST_JUNIT.as_bytes(), &[]);

    let price = "src/price.test.ts";
    assert_eq!(tables.diagnostics, vec![]);
    assert_eq!(
        tables.runs,
        vec![
            run(
                report,
                "src/basket.test.ts",
                &["basket", "settles on the second attempt"],
                RunOutcome::Passed,
                Some(12),
                Some(0),
            ),
            run(
                report,
                price,
                &["price", "handles currencies"],
                RunOutcome::Skipped,
                Some(0),
                None,
            ),
            run(
                report,
                price,
                &["price", "is parked"],
                RunOutcome::Skipped,
                Some(0),
                None,
            ),
            run(
                report,
                price,
                &["price", "round", "keeps two decimals"],
                RunOutcome::Failed,
                Some(3),
                Some(0),
            ),
            run(
                report,
                price,
                &["price", "round", "rounds half up"],
                RunOutcome::Passed,
                Some(2),
                Some(0),
            ),
            run(
                report,
                price,
                &["sits at the top level"],
                RunOutcome::Passed,
                Some(0),
                Some(0),
            ),
        ]
    );
}

#[test]
fn a_junit_report_with_no_testcase_yields_no_rows_and_no_diagnostics() {
    let reports = [
        r#"<testsuites name="vitest tests" tests="0" failures="0" errors="0" time="0"></testsuites>"#,
        r#"<testsuite name="src/price.test.ts" timestamp="2026-10-01T10:00:00.000Z" tests="0"/>"#,
    ];
    for report in reports {
        let tables = read_junit_xml(&context("report.xml"), report.as_bytes(), &[]);

        assert_eq!(tables.runs, vec![], "{report}");
        assert_eq!(tables.diagnostics, vec![], "{report}");
    }
}

#[test]
fn junit_xml_that_does_not_parse_is_one_diagnostic_and_no_rows() {
    let report = "reports/run-1.xml";
    // A report cut off mid-write, then no bytes at all.
    let truncated = &VITEST_JUNIT.as_bytes()[..VITEST_JUNIT.len() / 2];
    for bytes in [truncated, b"".as_slice()] {
        let tables = read_junit_xml(&context(report), bytes, &[]);

        assert_unreadable(&tables, report, "JUnit XML");
    }
}

/// A `testcase` with no `classname` names no file, and one whose file is outside the root
/// has no in-tree name: each is a diagnostic, never a guessed file, and the cases around
/// them still read.
#[test]
fn a_junit_testcase_without_a_file_in_the_tree_is_a_diagnostic_and_the_others_still_read() {
    let report = r#"<testsuite name="price" tests="4">
  <testcase classname="/work/shop/src/price.test.ts" name="rounds" time="0.001"/>
  <testcase name="price &gt; has no file" time="0.001"/>
  <testcase time="0.001"/>
  <testcase classname="/work/shop-old/src/price.test.ts" name="is elsewhere" time="0.001"/>
</testsuite>"#;
    let tables = read_junit_xml(&context("reports/run-1.xml"), report.as_bytes(), &[]);

    assert_eq!(
        outcomes(&tables.runs),
        vec![("src/price.test.ts", "rounds", RunOutcome::Passed, Some(0))]
    );
    assert_eq!(
        tables.diagnostics,
        vec![
            warning(
                "run_report_testcase_without_file",
                "reports/run-1.xml",
                "reports/run-1.xml has a testcase \"price > has no file\" with no classname; it \
                 is not recorded",
            ),
            warning(
                "run_report_testcase_without_file",
                "reports/run-1.xml",
                "reports/run-1.xml has a testcase \"\" with no classname; it is not recorded",
            ),
            warning(
                "run_report_path_outside_root",
                "reports/run-1.xml",
                "reports/run-1.xml names /work/shop-old/src/price.test.ts, which is not under \
                 /work/shop; its 1 results are not recorded",
            ),
        ]
    );
}

/// A failed case has one `failure` or `error` per failed attempt, as vitest writes them,
/// so it took one retry less than it has; an `error` fails a case as a `failure` does.
/// The third case, a `skipped` and a `failure` on one testcase, is one no known writer
/// emits: the failure wins.
#[test]
fn a_junit_case_with_a_failure_or_an_error_failed_and_took_one_retry_less_than_it_has_of_them() {
    let report = r#"<testsuite name="src/price.test.ts" tests="3">
  <testcase classname="src/price.test.ts" name="a threw">
    <error message="boom" type="TypeError"/>
  </testcase>
  <testcase classname="src/price.test.ts" name="b failed three times">
    <failure message="first" type="AssertionError"/>
    <failure message="second" type="AssertionError"/>
    <failure message="third" type="AssertionError"/>
  </testcase>
  <testcase classname="src/price.test.ts" name="c failed and was skipped">
    <skipped/>
    <failure message="first" type="AssertionError"/>
  </testcase>
</testsuite>"#;
    let tables = read_junit_xml(&context("report.xml"), report.as_bytes(), &[]);

    let file = "src/price.test.ts";
    assert_eq!(
        outcomes(&tables.runs),
        vec![
            (file, "a threw", RunOutcome::Failed, Some(0)),
            (file, "b failed three times", RunOutcome::Failed, Some(2)),
            (
                file,
                "c failed and was skipped",
                RunOutcome::Failed,
                Some(0)
            ),
        ]
    );
}

/// Retry children with Maven Surefire's element names, which vitest's `JUnit` never
/// writes: `flakyFailure`/`flakyError` or `rerunFailure`/`rerunError` without a final
/// `failure` mean the case passed on a retry; with a final `failure` every attempt failed.
/// Two of the cases are ones Surefire itself does not write: a `rerunFailure` with no final
/// `failure` (case b), and a `skipped` beside a `rerunFailure` (case d).
#[test]
fn junit_retry_children_read_as_passed_on_retry_unless_the_case_still_failed() {
    let report = r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuite name="src/retry.test.ts" tests="5">
  <testcase classname="src/retry.test.ts" name="retry &gt; a flaky" time="0.5">
    <flakyFailure message="first" type="AssertionError"/>
  </testcase>
  <testcase classname="src/retry.test.ts" name="retry &gt; b flaky twice" time="0.5">
    <rerunFailure message="first" type="AssertionError"/>
    <flakyError message="second" type="Error"/>
    <system-out>second attempt</system-out>
  </testcase>
  <testcase classname="src/retry.test.ts" name="retry &gt; c red" time="0.5">
    <failure message="last" type="AssertionError"/>
    <rerunFailure message="first" type="AssertionError"/>
    <rerunError message="second" type="Error"/>
  </testcase>
  <testcase classname="src/retry.test.ts" name="retry &gt; d skipped" time="0">
    <skipped/>
    <rerunFailure message="first" type="AssertionError"/>
  </testcase>
  <testcase classname="src/retry.test.ts" name="retry &gt; e green" time="0.5">
    <system-out>fine</system-out>
  </testcase>
</testsuite>"#;
    let tables = read_junit_xml(&context("surefire.xml"), report.as_bytes(), &[]);

    let file = "src/retry.test.ts";
    assert_eq!(
        outcomes(&tables.runs),
        vec![
            (file, "retry > a flaky", RunOutcome::PassedOnRetry, Some(1)),
            (
                file,
                "retry > b flaky twice",
                RunOutcome::PassedOnRetry,
                Some(2)
            ),
            (file, "retry > c red", RunOutcome::Failed, Some(2)),
            (file, "retry > d skipped", RunOutcome::Skipped, None),
            (file, "retry > e green", RunOutcome::Passed, Some(0)),
        ]
    );
}

/// The start of the run a report gives when its one suite is stamped `timestamp`, and the
/// duration of its one case when the case's `time` is `time`.
fn junit_start_and_duration(timestamp: &str, time: &str) -> (Option<u64>, Option<u64>) {
    let report = format!(
        r#"<testsuite name="src/price.test.ts" timestamp="{timestamp}" tests="1">
  <testcase classname="src/price.test.ts" name="rounds" time="{time}"/>
</testsuite>"#
    );
    let tables = read_junit_xml(&context("report.xml"), report.as_bytes(), &[]);
    (tables.runs[0].started_at_ms, tables.runs[0].duration_ms)
}

#[test]
fn a_junit_suite_timestamp_in_utc_reads_as_milliseconds_since_the_unix_epoch() {
    let stamps = [
        ("1970-01-01T00:00:00Z", 0),
        ("1970-01-01T00:00:00.007Z", 7),
        ("2000-01-31T12:30:45Z", 949_321_845_000),
        ("2001-12-31T01:02:03.5Z", 1_009_760_523_500),
        ("2024-02-29T23:59:59.999Z", 1_709_251_199_999),
        ("2024-03-01T00:00:00.0004Z", 1_709_251_200_000),
        ("2026-10-01T10:00:00.25Z", STARTED + 250),
    ];
    for (stamp, ms) in stamps {
        assert_eq!(junit_start_and_duration(stamp, "0").0, Some(ms), "{stamp}");
    }
}

/// A timestamp with no `Z` names no zone, and one that is not a date and a time names no
/// instant: the rows are kept with no start.
#[test]
fn a_junit_suite_timestamp_the_reader_cannot_place_leaves_the_rows_without_a_start() {
    let stamps = [
        "2026-10-01T10:00:00",
        "2026-10-01T10:00:00+02:00",
        "2026-10-01 10:00:00Z",
        "2026-10-01T10:00Z",
        "2026-10T10:00:00Z",
        "2026-10-01T10:00:00.12xZ",
        "yesterdayZ",
        "1969-12-31T23:59:59Z",
        "",
    ];
    for stamp in stamps {
        assert_eq!(junit_start_and_duration(stamp, "0").0, None, "{stamp:?}");
    }
}

/// A negative or non-finite `time` (`NaN` and `inf` parse as numbers) reads as a duration
/// of zero, not as no duration; only a `time` that parses as no number reads as none.
/// Current behaviour, pinned so a change is a decision and not an endorsement; see #27.
#[test]
fn junit_seconds_read_as_whole_milliseconds_a_negative_or_non_finite_time_as_zero_a_word_as_none() {
    let times = [
        ("0.0026", Some(3)),
        ("0.0024", Some(2)),
        ("1.5", Some(1500)),
        ("-1", Some(0)),
        ("NaN", Some(0)),
        ("inf", Some(0)),
        ("fast", None),
    ];
    for (time, ms) in times {
        assert_eq!(
            junit_start_and_duration("2026-10-01T10:00:00Z", time).1,
            ms,
            "{time}"
        );
    }
}

/// Vitest (2.1.9 and 4 alike) records a failure outside the tests as a failed testcase
/// named after its file, written last in the suite of that name. In `hook.test.ts` a hook
/// threw: the file gets a file-level row and the test it prevented, reported as skipped
/// before the file's own case, reads as failed. `load.test.ts` failed at import: its suite
/// holds only the file's own case. In `steady.test.ts` nothing failed outside the tests, so
/// the parked test stays skipped and a passing case that happens to be titled like the
/// file is a test like any other.
const FILE_LEVEL_JUNIT: &str = r#"<?xml version="1.0" encoding="UTF-8" ?>
<testsuites name="vitest tests" tests="6" failures="3" errors="0" time="0.02">
    <testsuite name="src/hook.test.ts" timestamp="2026-10-01T10:00:00.000Z" hostname="builder" tests="2" failures="1" errors="0" skipped="1" time="0.004">
        <testcase classname="src/hook.test.ts" name="reads the seeded rows" time="0">
            <skipped/>
        </testcase>
        <testcase classname="src/hook.test.ts" name="src/hook.test.ts" time="0.004">
            <failure message="seed failed" type="Error">
Error: seed failed
 ❯ src/hook.test.ts:2:25
            </failure>
        </testcase>
    </testsuite>
    <testsuite name="src/load.test.ts" timestamp="2026-10-01T10:00:00.000Z" hostname="builder" tests="1" failures="1" errors="0" skipped="0" time="0">
        <testcase classname="src/load.test.ts" name="src/load.test.ts" time="0">
            <failure message="Failed to load url ./missing (resolved id: ./missing) in /work/shop/src/load.test.ts. Does the file exist?" type="Error">
Error: Failed to load url ./missing (resolved id: ./missing) in /work/shop/src/load.test.ts. Does the file exist?
            </failure>
        </testcase>
    </testsuite>
    <testsuite name="src/steady.test.ts" timestamp="2026-10-01T10:00:00.000Z" hostname="builder" tests="3" failures="1" errors="0" skipped="1" time="0.003">
        <testcase classname="src/steady.test.ts" name="is parked" time="0">
            <skipped/>
        </testcase>
        <testcase classname="src/steady.test.ts" name="is red" time="0.002">
            <failure message="expected 1 to be 2 // Object.is equality" type="AssertionError">
AssertionError: expected 1 to be 2 // Object.is equality
 ❯ src/steady.test.ts:3:32
            </failure>
        </testcase>
        <testcase classname="src/steady.test.ts" name="src/steady.test.ts" time="0.001">
        </testcase>
    </testsuite>
</testsuites>
"#;

#[test]
fn a_junit_file_that_failed_outside_its_tests_gets_a_file_level_row() {
    let report = "report.xml";
    // A model test titled like the file: the file-level row must not join it.
    let model = [
        test_case("src/hook.test.ts", &[], "src/hook.test.ts", 10),
        test_case("src/steady.test.ts", &[], "src/steady.test.ts", 20),
    ];
    let tables = read_junit_xml(&context(report), FILE_LEVEL_JUNIT.as_bytes(), &model);

    assert_eq!(tables.diagnostics, vec![]);
    assert_eq!(
        tables.runs,
        vec![
            // The file-level row: an empty title path, failed, joined to nothing.
            run(
                report,
                "src/hook.test.ts",
                &[],
                RunOutcome::Failed,
                Some(4),
                Some(0),
            ),
            // Reported as skipped, but it never got to run.
            run(
                report,
                "src/hook.test.ts",
                &["reads the seeded rows"],
                RunOutcome::Failed,
                Some(0),
                None,
            ),
            // The file never loaded: its file-level row is all it leaves.
            run(
                report,
                "src/load.test.ts",
                &[],
                RunOutcome::Failed,
                Some(0),
                Some(0),
            ),
            run(
                report,
                "src/steady.test.ts",
                &["is parked"],
                RunOutcome::Skipped,
                Some(0),
                None,
            ),
            run(
                report,
                "src/steady.test.ts",
                &["is red"],
                RunOutcome::Failed,
                Some(2),
                Some(0),
            ),
            Run {
                joined: Some(JoinedCase {
                    file: "src/steady.test.ts".to_owned(),
                    span: Span::new(20, 21),
                }),
                ..run(
                    report,
                    "src/steady.test.ts",
                    &["src/steady.test.ts"],
                    RunOutcome::Passed,
                    Some(1),
                    Some(0),
                )
            },
        ]
    );
}

/// A failed case named after its file is the file's own failure only inside the suite of
/// that name: in a suite named otherwise it is a test, and its skipped sibling stays
/// skipped.
#[test]
fn a_failed_junit_case_named_after_its_file_in_a_suite_named_otherwise_is_a_test() {
    let report = r#"<testsuite name="unit" tests="2">
  <testcase classname="src/price.test.ts" name="src/price.test.ts">
    <failure message="expected 1 to be 2" type="AssertionError"/>
  </testcase>
  <testcase classname="src/price.test.ts" name="is parked">
    <skipped/>
  </testcase>
</testsuite>"#;
    let tables = read_junit_xml(&context("report.xml"), report.as_bytes(), &[]);

    let file = "src/price.test.ts";
    assert_eq!(
        outcomes(&tables.runs),
        vec![
            (file, "is parked", RunOutcome::Skipped, None),
            (file, "src/price.test.ts", RunOutcome::Failed, Some(0)),
        ]
    );
}

#[test]
fn a_junit_row_joins_the_test_with_its_file_and_title_path_and_an_unmatched_row_is_kept() {
    let tables = read_junit_xml(&context("report.xml"), VITEST_JUNIT.as_bytes(), &model());

    assert_eq!(
        joins(&tables.runs),
        vec![
            ("basket > settles on the second attempt", None),
            ("price > handles currencies", None),
            ("price > is parked", None),
            ("price > round > keeps two decimals", None),
            ("price > round > rounds half up", Some(&price_case_at(10))),
            ("sits at the top level", Some(&price_case_at(50))),
        ]
    );
}
