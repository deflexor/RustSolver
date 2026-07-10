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
mod range_parse;
mod cfr;
mod benchmark;

use benchmark::hu_turn_suite::{run_hu_turn_suite, suite_report_to_json};
use rust_poker::hand_evaluator::init_cards;

fn setup_out_dir() {
    if std::env::var("OUT_DIR").is_err() {
        let candidate = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target/release/deps");
        if candidate.join("offset_table.dat").exists() {
            std::env::set_var("OUT_DIR", &candidate);
        }
    }
}

fn main() {
    setup_out_dir();
    init_cards();

    let report = run_hu_turn_suite();
    let json = suite_report_to_json(&report);
    println!("{}", serde_json::to_string_pretty(&json).unwrap());
}
