//! Public deterministic entropy only. This fixture compares native Rust and actual WASM execution.
use ssc_protocol::Session;
fn main() {
    let mut a = Session::new("wasm-native-vector", true, [9; 32], [11; 32]).unwrap();
    let mut b = Session::new("wasm-native-vector", false, [17; 32], [19; 32]).unwrap();
    let ac = a.start().unwrap().remove(0);
    let bc = b.start().unwrap().remove(0);
    let ar = a.receive(&bc).unwrap().outgoing.remove(0);
    let br = b.receive(&ac).unwrap().outgoing.remove(0);
    a.receive(&br).unwrap();
    b.receive(&ar).unwrap();
    let sas = a.sas().unwrap().to_owned();
    assert_eq!(Some(sas.as_str()), b.sas());
    let ready_a = a.confirm().unwrap();
    let ready_b = b.confirm().unwrap();
    a.receive(&ready_b).unwrap();
    b.receive(&ready_a).unwrap();
    let text = a.send_text("Привет из WASM 🔒", [7; 16]).unwrap().remove(0);
    let event = a
        .send_event(ssc_protocol::ChatEvent::Reaction {
            target: "07".repeat(16),
            emoji: Some("🔥".into()),
        })
        .unwrap();
    println!(
        "{}",
        serde_json::json!({"commit_a":hex::encode(ac),"commit_b":hex::encode(bc),"reveal_a":hex::encode(ar),"reveal_b":hex::encode(br),"sas":sas,"ready_a":hex::encode(ready_a),"ready_b":hex::encode(ready_b),"text":hex::encode(text),"event":hex::encode(event)})
    );
}
