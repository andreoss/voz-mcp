use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::time::Duration;

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
    assert_eq!(out["path"], "/dev/null");

    child.kill().expect("kill");
    child.wait().expect("wait");
}
