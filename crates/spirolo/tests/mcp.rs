//! `spirolo mcp` over the real binary's stdin and stdout, compared with the real CLI's JSON.
//!
//! Each tool is asked for a document and the matching command is run on the same project;
//! the two must be equal. The project is `tests/fixtures/mcp-project`, a small Vitest
//! project written for these tests: one source module and one test file holding a sound
//! test, a weak one and a skipped one.

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use serde_json::{json, Value};

/// A private copy of the fixture project, named after the test that asked for it: some
/// tests write a config or a baseline into it.
fn fixture(name: &str) -> PathBuf {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("mcp-{name}"));
    let _ = std::fs::remove_dir_all(&root);
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mcp-project");
    for file in ["package.json", "src/price.ts", "src/price.test.ts"] {
        let to = root.join(file);
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        std::fs::copy(source.join(file), to).unwrap();
    }
    root
}

/// The source module of the fixture project, as `impact` names a changed file.
const SOURCE: &str = "src/price.ts";

/// A config outside the place `check` looks by default, raising one rule's severity so a
/// document read with it differs from one read without.
fn gate_config(root: &Path) -> PathBuf {
    let config = root.join("settings/gate.jsonc");
    std::fs::create_dir_all(config.parent().unwrap()).unwrap();
    std::fs::write(
        &config,
        r#"{ "rules": { "dead-skip": { "severity": "warn" } } }"#,
    )
    .unwrap();
    config
}

/// The JSON document a command prints when run in `root`.
fn cli(args: &[&str], root: &Path) -> Value {
    let output = Command::cargo_bin("spirolo")
        .unwrap()
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    assert_ne!(
        output.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn ratchet(root: &Path) {
    Command::cargo_bin("spirolo")
        .unwrap()
        .arg("ratchet")
        .arg(root)
        .assert()
        .success();
}

fn initialize() -> Value {
    json!({"jsonrpc":"2.0", "id":1, "method":"initialize", "params":{
        "protocolVersion":"2025-06-18", "capabilities":{},
        "clientInfo":{"name":"mcp-test", "version":"1"}
    }})
}

fn tools_call(id: u32, name: &str, arguments: &Value) -> Value {
    json!({"jsonrpc":"2.0", "id":id, "method":"tools/call", "params":{"name":name, "arguments":arguments}})
}

/// Send every message, close stdin, and return every response line.
fn exchange(messages: &[Value]) -> Vec<Value> {
    use std::fmt::Write as _;
    let mut input = String::new();
    for message in messages {
        writeln!(input, "{message}").unwrap();
    }
    exchange_raw(&input)
}

fn exchange_raw(input: &str) -> Vec<Value> {
    let output = Command::cargo_bin("spirolo")
        .unwrap()
        .arg("mcp")
        .timeout(std::time::Duration::from_secs(30))
        .write_stdin(input)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

/// The result of one tool call made after the handshake.
fn call(name: &str, arguments: &Value) -> Value {
    let responses = exchange(&[
        initialize(),
        json!({"jsonrpc":"2.0", "method":"notifications/initialized"}),
        tools_call(2, name, arguments),
    ]);
    assert_eq!(responses.len(), 2);
    assert_eq!(responses[1]["id"], 2);
    assert!(responses[1].get("error").is_none(), "{}", responses[1]);
    responses[1]["result"].clone()
}

/// The document a successful tool result carries as its one text block.
fn document(result: &Value) -> Value {
    assert!(result.get("isError").is_none(), "{result}");
    assert_eq!(result["content"].as_array().unwrap().len(), 1);
    assert_eq!(result["content"][0]["type"], "text");
    serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap()
}

// ------------------------------------------------------------------------- the handshake

#[test]
fn initialize_names_the_server_and_offers_tools() {
    let responses = exchange(&[initialize()]);
    assert_eq!(
        responses,
        [json!({"jsonrpc":"2.0", "id":1, "result":{
            "protocolVersion":"2025-06-18",
            "capabilities":{"tools":{}},
            "serverInfo":{"name":"spirolo", "version":env!("CARGO_PKG_VERSION")}
        }})]
    );
}

#[test]
fn initialize_keeps_a_supported_older_version_and_answers_an_unknown_one_with_the_latest() {
    for (asked, answered) in [
        ("2024-11-05", "2024-11-05"),
        ("2025-03-26", "2025-03-26"),
        ("unknown-version", "2025-06-18"),
    ] {
        let mut request = initialize();
        request["params"]["protocolVersion"] = json!(asked);
        let responses = exchange(&[request]);
        assert_eq!(responses[0]["result"]["protocolVersion"], answered);
    }
}

#[test]
fn notifications_get_no_response() {
    let responses = exchange(&[
        json!({"jsonrpc":"2.0", "method":"notifications/initialized"}),
        json!({"jsonrpc":"2.0", "method":"notifications/no-such-notification"}),
        json!({"jsonrpc":"2.0", "id":"after", "method":"ping"}),
    ]);
    assert_eq!(
        responses,
        [json!({"jsonrpc":"2.0", "id":"after", "result":{}})]
    );
}

#[test]
fn tools_list_describes_the_five_tools() {
    let responses = exchange(&[
        initialize(),
        json!({"jsonrpc":"2.0", "id":"discover", "method":"tools/list"}),
    ]);
    assert_eq!(responses[1]["id"], "discover");
    let tools = responses[1]["result"]["tools"].as_array().unwrap();
    let names: Vec<_> = tools
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["check", "explain", "impact", "list", "rule_status"]);
    for tool in tools {
        assert!(!tool["description"].as_str().unwrap().is_empty());
        assert_eq!(tool["inputSchema"]["type"], "object");
    }
    let properties = |tool: &Value| -> Vec<String> {
        let mut names: Vec<_> = tool["inputSchema"]["properties"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        names.sort();
        names
    };
    assert_eq!(properties(&tools[0]), ["all", "config", "root", "rule"]);
    assert_eq!(properties(&tools[1]), ["config", "query", "root"]);
    assert_eq!(properties(&tools[2]), ["files", "root"]);
    assert_eq!(properties(&tools[3]), ["root"]);
    assert!(properties(&tools[4]).is_empty());
    assert_eq!(tools[1]["inputSchema"]["required"], json!(["query"]));
    assert_eq!(tools[2]["inputSchema"]["required"], json!(["files"]));
}

#[test]
fn ping_answers_with_an_empty_result_under_the_request_id() {
    let responses = exchange(&[
        json!({"jsonrpc":"2.0", "id":"keepalive", "method":"ping"}),
        json!({"jsonrpc":"2.0", "id":-7, "method":"ping"}),
        json!({"jsonrpc":"2.0", "id":u64::MAX, "method":"ping"}),
    ]);
    assert_eq!(
        responses,
        [
            json!({"jsonrpc":"2.0", "id":"keepalive", "result":{}}),
            json!({"jsonrpc":"2.0", "id":-7, "result":{}}),
            json!({"jsonrpc":"2.0", "id":u64::MAX, "result":{}}),
        ]
    );
}

// ------------------------------------------- each tool returns its command's document

#[test]
fn check_returns_the_document_of_check_all() {
    let root = fixture("check");
    let expected = cli(&["check", "--all", "--format", "json"], &root);
    assert!(!expected["findings"].as_array().unwrap().is_empty());
    assert_eq!(
        document(&call("check", &json!({"root":root, "all":true}))),
        expected
    );
}

#[test]
fn check_shows_the_default_view_unless_asked_for_all() {
    let root = fixture("check-default");
    let all = cli(&["check", "--all", "--format", "json"], &root);
    let default = cli(&["check", "--format", "json"], &root);
    assert_ne!(
        all, default,
        "the fixture has findings of a rule hidden by default"
    );
    assert_eq!(document(&call("check", &json!({"root":root}))), default);
    assert_eq!(
        document(&call("check", &json!({"root":root, "all":false}))),
        default
    );
}

#[test]
fn check_takes_a_config_and_a_rule_like_the_command() {
    let root = fixture("check-config-rule");
    let config = gate_config(&root);
    let config_arg = config.to_str().unwrap();
    let base = ["check", "--all", "--format", "json"];
    let plain = cli(&base, &root);

    let with_config = cli(&[&base[..], &["--config", config_arg]].concat(), &root);
    assert_ne!(with_config, plain, "the config changes the document");
    assert_eq!(
        document(&call(
            "check",
            &json!({"root":root, "all":true, "config":config})
        )),
        with_config
    );

    let with_rule = cli(&[&base[..], &["--rule", "dead-skip"]].concat(), &root);
    assert_ne!(with_rule, plain, "the rule filter changes the document");
    assert!(!with_rule["findings"].as_array().unwrap().is_empty());
    assert_eq!(
        document(&call(
            "check",
            &json!({"root":root, "all":true, "rule":"dead-skip"})
        )),
        with_rule
    );
}

#[test]
fn explain_returns_the_document_of_explain_for_a_finding_and_for_a_rule() {
    let root = fixture("explain");
    let checked = cli(&["check", "--all", "--format", "json"], &root);
    let id = checked["findings"][0]["id"].as_str().unwrap();
    let finding = cli(&["explain", id, "--format", "json"], &root);
    let rule = cli(&["explain", "weak-assertion", "--format", "json"], &root);
    assert_ne!(finding, rule);
    assert_eq!(
        document(&call("explain", &json!({"root":root, "query":id}))),
        finding
    );
    assert_eq!(
        document(&call(
            "explain",
            &json!({"root":root, "query":"weak-assertion"})
        )),
        rule
    );
}

#[test]
fn explain_takes_the_same_config_as_check() {
    let root = fixture("explain-config");
    let config = gate_config(&root);
    let config_arg = config.to_str().unwrap();
    let query = "dead-skip src/price.test.ts total › rounds to cents#0";
    let plain = cli(&["explain", query, "--format", "json"], &root);
    let expected = cli(
        &["explain", query, "--format", "json", "--config", config_arg],
        &root,
    );
    assert_ne!(expected, plain, "the config changes the document");
    assert_eq!(
        document(&call(
            "explain",
            &json!({"root":root, "query":query, "config":config})
        )),
        expected
    );
}

#[test]
fn impact_returns_the_document_of_impact_for_a_changed_source() {
    let root = fixture("impact");
    let expected = cli(&["impact", SOURCE, "--format", "json"], &root);
    assert!(!expected["selected"].as_array().unwrap().is_empty());
    assert_eq!(
        document(&call("impact", &json!({"root":root, "files":[SOURCE]}))),
        expected
    );
}

#[test]
fn list_returns_the_document_of_list() {
    let root = fixture("list");
    let expected = cli(&["list", "--format", "json"], &root);
    assert!(!expected["test_cases"].as_array().unwrap().is_empty());
    assert_eq!(document(&call("list", &json!({"root":root}))), expected);
}

#[test]
fn rule_status_returns_the_document_of_rule_status() {
    let root = fixture("rule-status");
    let expected = cli(&["rule-status", "--format", "json"], &root);
    assert!(expected["rules"]["weak-assertion"]["verdict"].is_string());
    assert_eq!(document(&call("rule_status", &json!({}))), expected);
}

#[test]
fn a_call_without_arguments_reads_the_servers_current_directory() {
    let root = fixture("no-arguments");
    let responses: Vec<Value> = {
        let output = Command::cargo_bin("spirolo")
            .unwrap()
            .arg("mcp")
            .current_dir(&root)
            .timeout(std::time::Duration::from_secs(30))
            .write_stdin(format!(
                "{}\n",
                json!({"jsonrpc":"2.0", "id":1, "method":"tools/call", "params":{"name":"list"}})
            ))
            .output()
            .unwrap();
        assert!(output.status.success());
        String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    };
    assert_eq!(responses.len(), 1);
    assert_eq!(
        document(&responses[0]["result"]),
        cli(&["list", "--format", "json"], &root)
    );
}

// --------------------------------------------- errors, and the request that follows them

#[test]
fn an_unknown_tool_is_a_protocol_error_and_the_next_request_still_succeeds() {
    let responses = exchange(&[
        initialize(),
        json!({"jsonrpc":"2.0", "id":2, "method":"tools/call", "params":{"name":"no-such-tool"}}),
        json!({"jsonrpc":"2.0", "id":3, "method":"tools/list"}),
    ]);
    assert_eq!(responses.len(), 3);
    assert_eq!(
        responses[1],
        json!({"jsonrpc":"2.0", "id":2, "error":{"code":-32602, "message":"unknown tool 'no-such-tool'"}})
    );
    assert_eq!(responses[2]["id"], 3);
    assert_eq!(responses[2]["result"]["tools"].as_array().unwrap().len(), 5);
}

#[test]
fn a_call_that_names_no_tool_is_a_protocol_error() {
    let responses = exchange(&[
        json!({"jsonrpc":"2.0", "id":2, "method":"tools/call", "params":{"arguments":{}}}),
        json!({"jsonrpc":"2.0", "id":3, "method":"tools/call"}),
    ]);
    assert_eq!(responses.len(), 2);
    for (response, id) in responses.iter().zip([2, 3]) {
        assert_eq!(
            *response,
            json!({"jsonrpc":"2.0", "id":id, "error":{"code":-32602, "message":"expected a tool name"}})
        );
    }
}

#[test]
fn invalid_arguments_are_protocol_errors_and_the_next_request_still_succeeds() {
    let cases = [
        ("rule_status", json!({"unexpected":true})),
        ("explain", json!({})),
        (
            "explain",
            json!({"query":"weak-assertion", "format":"human"}),
        ),
        ("impact", json!({})),
        ("impact", json!({"files":"a.ts"})),
        ("impact", json!({"files":[], "since":"main"})),
        ("list", json!({"root":false})),
        ("list", json!({"root":".", "all":true})),
        ("check", json!({"all":"yes"})),
        ("check", json!({"unknown":42})),
    ];
    let mut messages: Vec<Value> = cases
        .iter()
        .zip(1..)
        .map(|((name, arguments), id)| tools_call(id, name, arguments))
        .collect();
    messages.push(json!({"jsonrpc":"2.0", "id":"after", "method":"ping"}));
    let responses = exchange(&messages);
    assert_eq!(responses.len(), cases.len() + 1);
    for ((response, (name, arguments)), id) in responses.iter().zip(&cases).zip(1..) {
        assert_eq!(response["id"], id, "{name} {arguments}");
        assert_eq!(response["error"]["code"], -32602, "{name} {arguments}");
        assert!(
            !response["error"]["message"].as_str().unwrap().is_empty(),
            "{name} {arguments}"
        );
        assert!(response.get("result").is_none(), "{name} {arguments}");
    }
    assert_eq!(
        responses[cases.len()],
        json!({"jsonrpc":"2.0", "id":"after", "result":{}})
    );
}

#[test]
fn malformed_json_is_a_parse_error_and_the_next_request_still_succeeds() {
    let responses = exchange_raw(&format!("{{bad json\n{}\n", initialize()));
    assert_eq!(responses.len(), 2);
    assert_eq!(
        responses[0],
        json!({"jsonrpc":"2.0", "id":null, "error":{"code":-32700, "message":"Parse error"}})
    );
    assert_eq!(responses[1]["id"], 1);
    assert_eq!(responses[1]["result"]["serverInfo"]["name"], "spirolo");
}

#[test]
fn requests_that_are_not_json_rpc_are_invalid_and_the_next_request_still_succeeds() {
    let invalid = |id: Value| json!({"jsonrpc":"2.0", "id":id, "error":{"code":-32600, "message":"Invalid Request"}});
    let responses = exchange(&[
        json!(null),
        json!({"jsonrpc":"1.0", "id":4, "method":"tools/list"}),
        json!({"id":5, "method":"tools/list"}),
        json!({"jsonrpc":"2.0", "id":"six", "method":7}),
        json!({"jsonrpc":"2.0", "id":"seven"}),
        json!({"jsonrpc":"2.0", "id":{}, "method":"tools/list"}),
        json!({"jsonrpc":"2.0", "id":1.5, "method":"tools/list"}),
        json!({"jsonrpc":"2.0", "id":true, "method":"tools/list"}),
        json!({"jsonrpc":"1.0", "method":"notifications/initialized"}),
        initialize(),
    ]);
    assert_eq!(
        responses[..9],
        [
            invalid(Value::Null),
            invalid(json!(4)),
            invalid(json!(5)),
            invalid(json!("six")),
            invalid(json!("seven")),
            invalid(Value::Null),
            invalid(Value::Null),
            invalid(Value::Null),
            invalid(Value::Null),
        ]
    );
    assert_eq!(responses.len(), 10);
    assert_eq!(responses[9]["result"]["serverInfo"]["name"], "spirolo");
}

#[test]
fn an_unknown_method_is_not_found_and_the_next_request_still_succeeds() {
    let responses = exchange(&[
        json!({"jsonrpc":"2.0", "id":8, "method":"resources/list"}),
        json!({"jsonrpc":"2.0", "id":9, "method":"ping"}),
    ]);
    assert_eq!(
        responses,
        [
            json!({"jsonrpc":"2.0", "id":8, "error":{"code":-32601, "message":"Method not found"}}),
            json!({"jsonrpc":"2.0", "id":9, "result":{}}),
        ]
    );
}

#[test]
fn analysis_errors_are_tool_errors_carrying_the_commands_message() {
    let root = fixture("errors");
    let missing = root.join("missing");
    let responses = exchange(&[
        initialize(),
        tools_call(2, "check", &json!({"root":missing})),
        tools_call(
            3,
            "explain",
            &json!({"root":root, "query":"no-such-finding"}),
        ),
        tools_call(4, "list", &json!({"root":missing})),
        tools_call(5, "impact", &json!({"root":missing, "files":[SOURCE]})),
        json!({"jsonrpc":"2.0", "id":6, "method":"tools/list"}),
    ]);
    assert_eq!(responses.len(), 6);
    let tool_error = |response: &Value| {
        assert!(response.get("error").is_none(), "{response}");
        assert_eq!(response["result"]["isError"], true, "{response}");
        assert_eq!(response["result"]["content"].as_array().unwrap().len(), 1);
        assert_eq!(response["result"]["content"][0]["type"], "text");
        response["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let not_a_directory = format!("{} is not a directory", missing.display());
    assert_eq!(tool_error(&responses[1]), not_a_directory);
    assert_eq!(
        tool_error(&responses[2]),
        "no finding with id 'no-such-finding'"
    );
    assert_eq!(tool_error(&responses[3]), not_a_directory);
    assert_eq!(tool_error(&responses[4]), not_a_directory);
    assert_eq!(responses[5]["result"]["tools"].as_array().unwrap().len(), 5);
}

// ------------------------------------------------------------------------ a live session

/// A server whose stdin stays open, as a real client holds it. Dropping it kills and reaps
/// the server.
struct Client {
    child: std::process::Child,
    responses: std::sync::mpsc::Receiver<String>,
}

impl Client {
    fn start(root: &Path) -> Self {
        use std::io::BufRead as _;
        let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_spirolo"))
            .arg("mcp")
            .current_dir(root)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let (sender, responses) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for line in std::io::BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        Self { child, responses }
    }

    fn send(&mut self, message: &Value) {
        use std::io::Write as _;
        let stdin = self.child.stdin.as_mut().unwrap();
        writeln!(stdin, "{message}").unwrap();
        stdin.flush().unwrap();
    }

    /// Send one request and wait for its response while stdin stays open.
    fn request(&mut self, message: &Value) -> Value {
        self.send(message);
        let response = self
            .responses
            .recv_timeout(std::time::Duration::from_secs(60))
            .expect("the server answers a request before stdin closes");
        serde_json::from_str(&response).unwrap()
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn a_live_session_answers_each_request_as_it_arrives_and_rereads_the_project() {
    let root = fixture("live");
    let mut client = Client::start(&root);
    assert_eq!(
        client.request(&initialize())["result"]["serverInfo"]["name"],
        "spirolo"
    );
    client.send(&json!({"jsonrpc":"2.0", "method":"notifications/initialized"}));
    let request = tools_call(7, "check", &json!({"all":true}));
    let first = client.request(&request);
    assert_eq!(first["id"], 7);
    let before = document(&first["result"]);
    assert_eq!(before, cli(&["check", "--all", "--format", "json"], &root));
    ratchet(&root);
    let after = document(&client.request(&request)["result"]);
    assert_eq!(after, cli(&["check", "--all", "--format", "json"], &root));
    assert_ne!(before, after);
}

#[test]
fn closing_stdin_ends_the_session_with_success() {
    let mut client = Client::start(&fixture("eof"));
    assert_eq!(
        client.request(&json!({"jsonrpc":"2.0", "id":1, "method":"ping"}))["id"],
        1
    );
    drop(client.child.stdin.take());
    assert_eq!(client.child.wait().unwrap().code(), Some(0));
}

#[test]
fn a_client_that_stops_reading_ends_the_session_quietly() {
    use std::io::Read as _;
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_spirolo"))
        .arg("mcp")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdout.take());
    {
        use std::io::Write as _;
        let mut stdin = child.stdin.take().unwrap();
        writeln!(stdin, "{}", initialize()).unwrap();
    }
    let mut stderr = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut stderr)
        .unwrap();
    assert_eq!(child.wait().unwrap().code(), Some(0), "{stderr}");
    assert_eq!(stderr, "");
}
