use unleash::interchange::{claude, opencode, hub::*};

fn main() {
    let raw = std::fs::read_to_string("src/interchange/tests/fixtures/claude-session.json").unwrap();
    let hub = claude::to_hub(std::io::BufReader::new(raw.as_bytes())).unwrap();
    let oc = opencode::from_hub(&hub).unwrap();
    for (i, msg) in oc.messages.iter().enumerate() {
        println!("msg_i: {} role: {}", i, msg.get("role").unwrap());
    }
    for (i, part) in oc.parts.iter().enumerate() {
        println!("part_i: {} type: {} _msg_idx: {:?}", i, part.get("type").unwrap(), part.get("_msg_idx"));
    }
}
