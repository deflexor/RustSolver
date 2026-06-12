#![allow(dead_code)]
#![allow(unused_imports)]

mod constants;
mod state;
mod tree;
mod nodes;
mod actions;
mod options;
mod tree_builder;
mod card_abstraction;
mod infoset;
mod cfr;

use cfr::MCCFRTrainer;
use rust_poker::hand_evaluator::init_cards;
use std::time::Instant;

fn main() {
    // rust_poker 0.1.5's evaluator reads `offset_table.dat` from
    // `OUT_DIR`, which is a build-time-only env var. The build script
    // writes the file to `target/release/deps/`. Set OUT_DIR at runtime
    // so the evaluator can find it.
    if std::env::var("OUT_DIR").is_err() {
        let candidate = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target/release/deps");
        if candidate.join("offset_table.dat").exists() {
            std::env::set_var("OUT_DIR", &candidate);
        }
    }

    // Pre-init rust_poker's lazy_static CARDS and LOOKUP_TABLE on the
    // main thread before spawning worker threads. The 0.1.5 lazy_static
    // implementation panics on concurrent first access.
    init_cards();

    let options = options::default_flop();
    let mut trainer = MCCFRTrainer::init(options);
    let start = Instant::now();
    trainer.train(10_000_000);
    let elapsed = start.elapsed().subsec_nanos();
    println!("{}", elapsed);
}
