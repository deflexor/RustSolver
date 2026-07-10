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

use cfr::{MCCFRTrainer, TrainConfig};
use crate::options::{self as solver_options, SolverPreset};
use rust_poker::hand_evaluator::init_cards;
use std::time::Instant;

/// Parse CLI flags. Default preset is turn-entry (`default_turn_solve`).
fn parse_cli() -> (TrainConfig, SolverPreset) {
    let mut cfg = TrainConfig::default();
    let mut preset = SolverPreset::Turn;
    let args: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < args.len() {
        let arg = &args[i];
        match arg.as_str() {
            "--preset" => {
                if let Some(v) = args.get(i + 1) {
                    preset = match v.as_str() {
                        "turn" => SolverPreset::Turn,
                        "flop" => SolverPreset::Flop,
                        other => {
                            eprintln!(
                                "--preset must be `turn` or `flop` (got {:?})",
                                other
                            );
                            std::process::exit(2);
                        }
                    };
                    i += 2;
                    continue;
                }
                eprintln!("--preset requires `turn` or `flop`");
                std::process::exit(2);
            }
            "--max-iter" => {
                if let Some(v) = args.get(i + 1) {
                    if let Ok(n) = v.parse::<usize>() {
                        cfg = cfg.with_max_iter(n);
                        i += 2;
                        continue;
                    }
                }
                eprintln!("--max-iter requires a positive integer");
                std::process::exit(2);
            }
            "--target-mbb" => {
                if let Some(v) = args.get(i + 1) {
                    if let Ok(m) = v.parse::<f32>() {
                        cfg = cfg.with_target_exploitability_mbb(m);
                        i += 2;
                        continue;
                    }
                }
                eprintln!("--target-mbb requires a float");
                std::process::exit(2);
            }
            "--convergence-interval" => {
                if let Some(v) = args.get(i + 1) {
                    if let Ok(n) = v.parse::<usize>() {
                        cfg = cfg.with_convergence_interval(n);
                        i += 2;
                        continue;
                    }
                }
                eprintln!("--convergence-interval requires a positive integer");
                std::process::exit(2);
            }
            "--convergence-path" => {
                if let Some(v) = args.get(i + 1) {
                    cfg = cfg.with_convergence_path(v);
                    i += 2;
                    continue;
                }
                eprintln!("--convergence-path requires a path");
                std::process::exit(2);
            }
            "--cfr-plus" => {
                cfg = cfg.with_cfr_plus(true);
                i += 1;
            }
            "--no-cfr-plus" => {
                cfg = cfg.with_cfr_plus(false);
                i += 1;
            }
            "--help" | "-h" => {
                eprintln!(
                    "rust_solver [--preset turn|flop] [--max-iter N] [--target-mbb M] \\\n\
                     \x20            [--convergence-interval I] [--convergence-path P] \\\n\
                     \x20            [--cfr-plus | --no-cfr-plus]\n\
                     \n\
                     Defaults: --preset turn --max-iter 10000000 \\\n\
                     \x20          --convergence-interval 100000 \\\n\
                     \x20          --convergence-path convergence.jsonl --cfr-plus\n\
                     \n\
                     Flop preset uses full random ranges and may OOM until sparse\n\
                     infoset allocation (P5.2) lands. Turn preset is the safe default."
                );
                std::process::exit(0);
            }
            other => {
                eprintln!("unknown flag: {}", other);
                std::process::exit(2);
            }
        }
    }
    (cfg, preset)
}

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
    // main thread before spawning worker threads.
    init_cards();

    let (cfg, preset) = parse_cli();
    let options = match preset {
        SolverPreset::Turn => solver_options::default_turn_solve(),
        SolverPreset::Flop => {
            eprintln!(
                "warning: --preset flop uses full random ranges and may OOM;\n\
                 \x20        prefer --preset turn until sparse infosets (P5.2) land"
            );
            solver_options::default_flop()
        }
    };
    let mut trainer = MCCFRTrainer::init(options);
    let start = Instant::now();
    trainer.train_with_config(&cfg);
    let elapsed = start.elapsed();
    eprintln!(
        "done: {} iters in {:.2}s (preset={})",
        cfg.max_iter,
        elapsed.as_secs_f64(),
        preset.name()
    );
}
