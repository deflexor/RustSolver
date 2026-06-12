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
use std::time::Instant;

fn main() {
    let options = options::default_flop();
    let mut trainer = MCCFRTrainer::init(options);
    let start = Instant::now();
    trainer.train(10_000_000);
    let elapsed = start.elapsed().subsec_nanos();
    println!("{}", elapsed);
}
