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
mod benchmark;

use benchmark::kk_turn::{print_report, run_kk_turn_benchmark};
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

    let report = run_kk_turn_benchmark();
    let json_mode = std::env::args().any(|a| a == "--json");

    if json_mode {
        let json = serde_json::json!({
            "solver": report.solver,
            "stack_bucket_bb": report.stack_bucket_bb,
            "solve_elapsed_ms": report.solve_elapsed_ms,
            "total_samples": report.total_samples,
            "turn_boards_sampled": report.turn_boards_sampled,
            "matched_sample": report.matched_sample.as_ref().map(|m| serde_json::json!({
                "board": m.board,
                "pot_bb": m.pot_bb,
                "call_cost_bb": m.call_cost_bb,
                "action_probs": m.action_probs,
                "raise_probs": m.raise_probs,
            })),
            "decisions_ranked": report.decisions_ranked.iter().map(|d| serde_json::json!({
                "rank": d.rank,
                "action": d.action,
                "raise_to_bb": d.raise_to_bb,
                "score": d.score,
            })).collect::<Vec<_>>(),
        });
        println!("{}", serde_json::to_string_pretty(&json).unwrap());
    } else {
        print_report(&report);
    }
}
