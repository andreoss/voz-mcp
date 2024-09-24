use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::time::Duration;

use voz_mcp::wav::validate;

fn send_msg(w: &mut impl Write, msg: &str) {
    writeln!(w, "{msg}").expect("write");
    w.flush().expect("flush");
}

fn read_next(r: &mut impl BufRead) -> String {
    let deadline = std::time::Instant::now() + Duration::from_secs(90);
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
    let out_dir = std::env::temp_dir().join(format!("voz-e2e-speak-{}", std::process::id()));
    let mut child = Command::new(env!("CARGO_BIN_EXE_voz")).arg("mcp")
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
    std::fs::remove_dir_all(&out_dir).ok();
}

#[test]
fn neural_cli_yields_valid_wav_and_distinct_bytes_per_language() {
    let out_dir = std::env::temp_dir().join(format!("voz-e2e-neural-{}", std::process::id()));
    std::fs::create_dir_all(&out_dir).expect("mkdir out");
    let mut outputs = Vec::new();
    for lang in ["en", "ru", "es", "de", "ja"] {
        let out = out_dir.join(format!("{lang}.wav"));
        let status = Command::new(env!("CARGO_BIN_EXE_voz"))
            .arg("neural probe")
            .arg("--lang")
            .arg(lang)
            .arg("--out")
            .arg(&out)
            .env("VOZ_OUT_DIR", &out_dir)
            .stdout(Stdio::null())
            .env("VOZ_NEURAL_ROOT", std::env::var("VOZ_NEURAL_ROOT").unwrap_or_default())
            .status()
            .expect("run cli");
        assert!(status.success(), "cli failed for lang {lang}");
        outputs.push((lang, std::fs::read(&out).expect("read wav")));
    }
    for (lang, bytes) in &outputs {
        assert!(validate(bytes).is_ok(), "{lang} output not a valid wav");
    }
    for (i, (la, a)) in outputs.iter().enumerate() {
        for (lb, b) in outputs.iter().skip(i + 1) {
            assert_ne!(a, b, "{la} and {lb} must synthesize distinct audio");
        }
    }
    std::fs::remove_dir_all(&out_dir).ok();
}

fn collect_responses(r: &mut impl BufRead, count: usize) -> Vec<serde_json::Value> {
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        out.push(serde_json::from_str(&read_next(r)).expect("parse response"));
    }
    out
}

#[test]
fn pipelined_calls_yield_exact_id_set() {
    let out_dir =
        std::env::temp_dir().join(format!("voz-e2e-pipeline-{}", std::process::id()));
    let mut child = Command::new(env!("CARGO_BIN_EXE_voz")).arg("mcp")
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

    for id in [5, 4, 3, 2] {
        send_msg(
            &mut stdin,
            &format!(
                r#"{{"jsonrpc":"2.0","id":{id},"method":"tools/call","params":{{"name":"speak","arguments":{{"text":"pipeline {id}","lang":"en"}}}}}}"#
            ),
        );
    }
    let responses = collect_responses(&mut stdout, 4);
    let mut ids: Vec<i64> = responses
        .iter()
        .map(|r| serde_json::from_value::<serde_json::Value>(r["id"].clone()).unwrap())
        .map(|v| v.as_i64().unwrap())
        .collect();
    ids.sort_unstable();
    assert_eq!(ids, vec![2, 3, 4, 5], "response id set must match request id set");
    for r in &responses {
        assert_eq!(r["result"]["isError"], serde_json::Value::Bool(false));
        let text = r["result"]["content"][0]["text"].as_str().unwrap();
        let out: serde_json::Value = serde_json::from_str(text).expect("parse output");
        let path = out["path"].as_str().unwrap();
        assert!(
            std::path::Path::new(path).exists(),
            "speech file missing: {path}"
        );
    }

    child.kill().expect("kill");
    child.wait().expect("wait");
    std::fs::remove_dir_all(&out_dir).ok();
}

#[test]
fn server_speaks_with_rate_and_rejects_out_of_range_rate() {
    let out_dir = std::env::temp_dir().join(format!("voz-e2e-rate-{}", std::process::id()));
    let mut child = Command::new(env!("CARGO_BIN_EXE_voz")).arg("mcp")
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
    let mut child = Command::new(env!("CARGO_BIN_EXE_voz")).arg("mcp")
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
    let mut child = Command::new(env!("CARGO_BIN_EXE_voz")).arg("mcp")
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

#[test]
fn piper_cli_speaks_when_neural_absent_and_rejects_uncovered_language() {
    let piper_bin = std::path::PathBuf::from("/user/modelz/piper/bin/piper");
    if !piper_bin.exists() {
        return;
    }
    let fake_root = std::env::temp_dir().join(format!("voz-e2e-piper-root-{}", std::process::id()));
    let out_dir = std::env::temp_dir().join(format!("voz-e2e-piper-out-{}", std::process::id()));
    std::fs::create_dir_all(&fake_root).expect("mkdir root");
    std::fs::create_dir_all(&out_dir).expect("mkdir out");
    let mut outputs = Vec::new();
    for lang in ["en", "ru", "es"] {
        let out = out_dir.join(format!("{lang}.wav"));
        let status = Command::new(env!("CARGO_BIN_EXE_voz"))
            .arg("fallback probe")
            .arg("--lang")
            .arg(lang)
            .arg("--rate")
            .arg("200")
            .arg("--out")
            .arg(&out)
            .env("VOZ_OUT_DIR", &out_dir)
            .env("VOZ_NEURAL_ROOT", &fake_root)
            .env("VOZ_PIPER_BIN", &piper_bin)
            .stdout(Stdio::null())
            .status()
            .expect("run cli");
        assert!(status.success(), "cli failed for lang {lang}");
        outputs.push((lang, std::fs::read(&out).expect("read wav")));
    }
    for (lang, bytes) in &outputs {
        let info = validate(bytes).expect("valid wav");
        assert_eq!(info.audio_format, 1, "{lang} not pcm");
        assert_eq!(info.sample_rate, 22050, "{lang} wrong sample rate");
        assert!(info.data_size > 20000, "{lang} suspiciously short");
    }
    for (i, (la, a)) in outputs.iter().enumerate() {
        for (lb, b) in outputs.iter().skip(i + 1) {
            assert_ne!(a, b, "{la} and {lb} must synthesize distinct audio");
        }
    }
    let status = Command::new(env!("CARGO_BIN_EXE_voz"))
        .arg("fallback probe")
        .arg("--lang")
        .arg("ja")
        .env("VOZ_OUT_DIR", &out_dir)
        .env("VOZ_NEURAL_ROOT", &fake_root)
        .env("VOZ_PIPER_BIN", &piper_bin)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("run cli");
    assert!(!status.success(), "ja must fail without a voice");
    std::fs::remove_dir_all(&fake_root).ok();
    std::fs::remove_dir_all(&out_dir).ok();
}
