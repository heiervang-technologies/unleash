fn main() {
    if let Err(err) = unleash::run() {
        let msg = err.to_string();
        if !msg.is_empty() {
            eprintln!("\x1b[31merror:\x1b[0m {}", msg);
        }
        std::process::exit(1);
    }
}
