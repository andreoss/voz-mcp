fn main() {
    let path = std::env::args().nth(1).expect("usage: wav_check <path>");
    let bytes = std::fs::read(&path).expect("read wav file");
    match voz_mcp::wav::validate(&bytes) {
        Ok(_) => println!("ok"),
        Err(e) => {
            println!("fail: {e:?}");
            std::process::exit(1);
        }
    }
}
