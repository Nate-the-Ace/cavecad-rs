fn main() {
    let t = std::fs::read_to_string(std::env::args().nth(1).unwrap()).unwrap();
    let ops: Vec<cave_doc::Op> = match serde_json::from_str(&t) { Ok(o) => o, Err(e) => { println!("parse error: {e}"); return; } };
    let mut doc = cave_doc::Document::new(cave_doc::ActorId(1));
    for (i, op) in ops.iter().enumerate() { if let Err(e) = doc.apply(op) { println!("apply error at {i}: {e:?} {op:?}"); return; } }
    println!("ok {}", ops.len());
}
