fn main() {
    let path = std::env::args().nth(1).expect("usage: wav_check <path>");
    let bytes = std::fs::read(&path).expect("read wav file");
    match voz_mcp::wav::measure(&bytes) {
        Ok(m) if m.is_silent() => {
            println!(
                "fail: silent {:.3}s peak {} rms {:.1}",
                m.seconds, m.peak, m.rms
            );
            std::process::exit(1);
        }
        Ok(m) => println!("ok {:.3}s peak {} rms {:.1}", m.seconds, m.peak, m.rms),
        Err(e) => {
            println!("fail: {e:?}");
            std::process::exit(1);
        }
    }
}
