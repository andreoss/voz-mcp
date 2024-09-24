use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::time::Duration;

use voz_mcp::wav::validate;

fn send_msg(w: &mut impl Write, msg: &str) {
    writeln!(w, "{msg}").expect("write");
    w.flush().expect("flush");
}

fn read_next(r: &mut impl BufRead) -> String {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        let mut line = String::new();
        let n = r.read_line(&mut line).expect("read");
        if n == 0 {
            return line;
        }
        if !line.trim().is_empty() {
            return line;
        }
        if std::time::Instant::now() > deadline {
            panic!("timed out waiting for server output");
        }
    }
}

#[test]
fn server_speaks_over_stdio() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_voz-mcp"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn server");

    let mut stdin = child.stdin.take().expect("stdin");
    let mut stdout = BufReader::new(child.stdout.take().expect("stdout"));

    send_msg(
        &mut stdin,
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"e2e","version":"0.0.0"}}}"#,
    );
    let init: serde_json::Value = serde_json::from_str(&read_next(&mut stdout)).expect("parse init");
    assert_eq!(init["result"]["serverInfo"]["name"], "voz-mcp");

    send_msg(&mut stdin, r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#);

    send_msg(
        &mut stdin,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#,
    );
    let tools: serde_json::Value = serde_json::from_str(&read_next(&mut stdout)).expect("parse tools");
    let names: Vec<&str> = tools["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"speak"));

    send_msg(
        &mut stdin,
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"speak","arguments":{"text":"hola","lang":"es"}}}"#,
    );
    let call: serde_json::Value = serde_json::from_str(&read_next(&mut stdout)).expect("parse call");
    assert_eq!(call["result"]["isError"], serde_json::Value::Bool(false));
    let text = call["result"]["content"][0]["text"].as_str().unwrap();
    let out: serde_json::Value = serde_json::from_str(text).expect("parse output");
    assert_eq!(out["lang"], "es");
    let path = out["path"].as_str().unwrap();
    let file = std::path::Path::new(path);
    assert!(file.exists(), "speech file missing: {path}");
    assert!(
        validate(&std::fs::read(file).unwrap()).is_ok(),
        "not a valid wav: {path}"
    );

    child.kill().expect("kill");
    child.wait().expect("wait");
}

#[test]
fn server_speaks_with_rate_and_rejects_out_of_range_rate() {
    let out_dir = std::env::temp_dir().join(format!("voz-e2e-rate-{}", std::process::id()));
    let mut child = Command::new(env!("CARGO_BIN_EXE_voz-mcp"))
        .env("VOZ_OUT_DIR", &out_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn server");

    let mut stdin = child.stdin.take().expect("stdin");
    let mut stdout = BufReader::new(child.stdout.take().expect("stdout"));

    send_msg(
        &mut stdin,
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"e2e","version":"0.0.0"}}}"#,
    );
    let _init = read_next(&mut stdout);

    send_msg(&mut stdin, r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#);

    send_msg(
        &mut stdin,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"speak","arguments":{"text":"hello","lang":"en","rate":150}}}"#,
    );
    let call: serde_json::Value = serde_json::from_str(&read_next(&mut stdout)).expect("parse call");
    assert_eq!(call["result"]["isError"], serde_json::Value::Bool(false));
    let text = call["result"]["content"][0]["text"].as_str().unwrap();
    let out: serde_json::Value = serde_json::from_str(text).expect("parse output");
    let path = out["path"].as_str().unwrap();
    let file = std::path::Path::new(path);
    assert!(file.exists(), "speech file missing: {path}");
    assert!(
        validate(&std::fs::read(file).unwrap()).is_ok(),
        "not a valid wav: {path}"
    );

    send_msg(
        &mut stdin,
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"speak","arguments":{"text":"hello","lang":"en","rate":9999}}}"#,
    );
    let bad_call: serde_json::Value = serde_json::from_str(&read_next(&mut stdout)).expect("parse bad call");
    assert_eq!(bad_call["error"]["code"], serde_json::Value::from(-32602));

    child.kill().expect("kill");
    child.wait().expect("wait");
    std::fs::remove_dir_all(&out_dir).ok();
}

#[test]
fn server_speaks_with_pitch_and_rejects_out_of_range_pitch() {
    let out_dir = std::env::temp_dir().join(format!("voz-e2e-pitch-{}", std::process::id()));
    let mut child = Command::new(env!("CARGO_BIN_EXE_voz-mcp"))
        .env("VOZ_OUT_DIR", &out_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn server");

    let mut stdin = child.stdin.take().expect("stdin");
    let mut stdout = BufReader::new(child.stdout.take().expect("stdout"));

    send_msg(
        &mut stdin,
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"e2e","version":"0.0.0"}}}"#,
    );
    let _init = read_next(&mut stdout);

    send_msg(&mut stdin, r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#);

    send_msg(
        &mut stdin,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"speak","arguments":{"text":"hi","lang":"en","pitch":60}}}"#,
    );
    let call: serde_json::Value = serde_json::from_str(&read_next(&mut stdout)).expect("parse call");
    assert_eq!(call["result"]["isError"], serde_json::Value::Bool(false));
    let text = call["result"]["content"][0]["text"].as_str().unwrap();
    let out: serde_json::Value = serde_json::from_str(text).expect("parse output");
    let path = out["path"].as_str().unwrap();
    let file = std::path::Path::new(path);
    assert!(file.exists(), "speech file missing: {path}");
    assert!(
        validate(&std::fs::read(file).unwrap()).is_ok(),
        "not a valid wav: {path}"
    );

    send_msg(
        &mut stdin,
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"speak","arguments":{"text":"hi","lang":"en","pitch":100}}}"#,
    );
    let bad_call: serde_json::Value = serde_json::from_str(&read_next(&mut stdout)).expect("parse bad call");
    assert_eq!(bad_call["error"]["code"], serde_json::Value::from(-32602));
    assert!(bad_call["error"]["message"].as_str().unwrap().contains("pitch"));

    child.kill().expect("kill");
    child.wait().expect("wait");
    std::fs::remove_dir_all(&out_dir).ok();
}

#[test]
fn server_lists_readback_after_speak() {
    let out_dir =
        std::env::temp_dir().join(format!("voz-e2e-readback-{}", std::process::id()));
    let mut child = Command::new(env!("CARGO_BIN_EXE_voz-mcp"))
        .env("VOZ_OUT_DIR", &out_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn server");

    let mut stdin = child.stdin.take().expect("stdin");
    let mut stdout = BufReader::new(child.stdout.take().expect("stdout"));

    send_msg(
        &mut stdin,
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"e2e","version":"0.0.0"}}}"#,
    );
    let _init = read_next(&mut stdout);

    send_msg(&mut stdin, r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#);

    send_msg(
        &mut stdin,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#,
    );
    let tools: serde_json::Value = serde_json::from_str(&read_next(&mut stdout)).expect("parse tools");
    let names: Vec<&str> = tools["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"speak"));
    assert!(names.contains(&"readback"));

    send_msg(
        &mut stdin,
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"speak","arguments":{"text":"listen back","lang":"en"}}}"#,
    );
    let speak: serde_json::Value = serde_json::from_str(&read_next(&mut stdout)).expect("parse speak call");
    assert_eq!(speak["result"]["isError"], serde_json::Value::Bool(false));
    let speak_text = speak["result"]["content"][0]["text"].as_str().unwrap();
    let speak_out: serde_json::Value = serde_json::from_str(speak_text).expect("parse speak output");
    let speak_path = speak_out["path"].as_str().unwrap();

    send_msg(
        &mut stdin,
        r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"readback","arguments":{}}}"#,
    );
    let rb: serde_json::Value = serde_json::from_str(&read_next(&mut stdout)).expect("parse readback call");
    assert_eq!(rb["result"]["isError"], serde_json::Value::Bool(false));
    let text = rb["result"]["content"][0]["text"].as_str().unwrap();
    let out: serde_json::Value = serde_json::from_str(text).expect("parse readback output");
    let items = out["items"].as_array().expect("items array");
    assert!(!items.is_empty(), "expected at least one recording listed");
    assert!(items.iter().any(|i| i["name"].as_str().unwrap().ends_with(".wav")));
    let listed = items
        .iter()
        .find(|i| i["path"].as_str() == Some(speak_path))
        .expect("spoken file listed in readback");
    assert!(
        validate(&std::fs::read(listed["path"].as_str().unwrap()).unwrap()).is_ok(),
        "not a valid wav: {speak_path}"
    );

    child.kill().expect("kill");
    child.wait().expect("wait");
    std::fs::remove_dir_all(&out_dir).ok();
}
