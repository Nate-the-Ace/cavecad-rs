//! The property that makes merging and later co-editing safe: any order, same result.

use cave_doc::{Action, ActorId, Document, Field, Kind, Op, Value};
use cave_id::{Id, IdSource, SeqIds};

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// Three editors make random edits on their own copies, then everything is exchanged.
fn random_ops(seed: u64) -> Vec<Op> {
    let mut rng = Rng(seed);
    let mut ids = SeqIds::new(7);
    let pool: Vec<Id> = (0..6).map(|_| ids.next_id()).collect();
    let mut all = Vec::new();
    for actor in 1..=3 {
        let mut d = Document::new(ActorId(actor));
        for _ in 0..25 {
            let id = pool[rng.below(pool.len())];
            let action = match rng.below(4) {
                0 => Action::Create {
                    id,
                    kind: Kind::Station,
                },
                1 => Action::Remove {
                    id,
                    kind: Kind::Station,
                },
                2 => Action::Set {
                    id,
                    field: Field::StationLabel,
                    value: Value::Text(format!("S{}", rng.below(5))),
                },
                _ => Action::Set {
                    id,
                    field: Field::StationFixedZ,
                    value: Value::Number(Some(rng.below(100) as f64)),
                },
            };
            all.extend(d.edit(vec![action]).unwrap().ops);
        }
    }
    all
}

fn shuffled(ops: &[Op], rng: &mut Rng) -> Vec<Op> {
    let mut v = ops.to_vec();
    for i in (1..v.len()).rev() {
        v.swap(i, rng.below(i + 1));
    }
    v
}

#[test]
fn any_order_gives_the_same_document() {
    for seed in 1..=40u64 {
        let ops = random_ops(seed);
        let mut rng = Rng(seed * 7919);
        let mut reference = Document::new(ActorId(99));
        for op in &ops {
            reference.apply(op).unwrap();
        }
        for _ in 0..10 {
            let mut d = Document::new(ActorId(98));
            for op in shuffled(&ops, &mut rng) {
                d.apply(&op).unwrap();
            }
            assert_eq!(d, reference, "seed {seed}");
        }
    }
}

#[test]
fn replaying_everything_twice_changes_nothing() {
    let ops = random_ops(5);
    let mut d = Document::new(ActorId(99));
    for op in &ops {
        d.apply(op).unwrap();
    }
    let once = d.clone();
    for op in &ops {
        d.apply(op).unwrap();
    }
    assert_eq!(d, once);
}
