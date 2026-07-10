//! Multi-spot HU turn benchmark suite (Phase 12.5).

use std::time::Instant;

use crate::cfr::{convergence, HeroDecisionSample, MCCFRTrainer, TrainConfig};
use crate::state::BettingRound;

use super::kk_turn::{
    hero_hole_cards, hero_hole_cards_from_str, options_for_hu_turn_spot, options_for_turn_card,
    pick_matching_sample, sample_to_decisions, RankedDecision, MAX_ITER, SAMPLE_TOLERANCE_BB,
    TRAIN_SEED,
};

#[derive(Debug, Clone)]
pub struct SuiteSpot {
    pub id: String,
    pub hero_hand: String,
    pub flop: String,
    pub turn_card: String,
    pub query_pot_bb: f32,
    pub query_call_bb: f32,
    pub stack_bb: u32,
    pub oop_range: String,
    pub ip_range: String,
    pub baseline_top_action: Option<String>,
    pub baseline_check_prob: Option<f32>,
    pub measure_exploitability: bool,
}

#[derive(Debug, Clone)]
pub struct SpotResult {
    pub spot_id: String,
    pub solve_elapsed_ms: f64,
    pub exploitability_max_mbb: Option<f32>,
    pub matched_sample: Option<HeroDecisionSample>,
    pub decisions_ranked: Vec<RankedDecision>,
    pub top_action: Option<String>,
    pub check_prob: Option<f32>,
}

#[derive(Debug, Clone)]
pub struct SuiteReport {
    pub spots: Vec<SpotResult>,
    pub total_elapsed_ms: f64,
    pub parity_matches: usize,
    pub parity_total: usize,
}

pub fn load_suite_spots() -> Vec<SuiteSpot> {
    let raw = include_str!("../../../benchmarks/hu_turn_suite.json");
    let doc: serde_json::Value = serde_json::from_str(raw).expect("hu_turn_suite.json");
    let spots = doc["spots"].as_array().expect("spots array");
    spots
        .iter()
        .map(|s| SuiteSpot {
            id: s["id"].as_str().unwrap().to_string(),
            hero_hand: s["hero_hand"].as_str().unwrap().to_string(),
            flop: s["flop"].as_str().unwrap().to_string(),
            turn_card: s["turn_card"].as_str().unwrap().to_string(),
            query_pot_bb: s["query_pot_bb"].as_f64().unwrap() as f32,
            query_call_bb: s["query_call_bb"].as_f64().unwrap() as f32,
            stack_bb: s["stack_bb"].as_u64().unwrap() as u32,
            oop_range: s["oop_range"].as_str().unwrap().to_string(),
            ip_range: s["ip_range"].as_str().unwrap().to_string(),
            baseline_top_action: s["baseline_top_action"]
                .as_str()
                .map(|x| x.to_string()),
            baseline_check_prob: s["baseline_check_prob"]
                .as_f64()
                .map(|x| x as f32),
            measure_exploitability: s
                .get("measure_exploitability")
                .and_then(|v| v.as_bool())
                .unwrap_or_else(|| s["id"].as_str().unwrap_or("").starts_with("kk_")),
        })
        .collect()
}

pub fn solve_suite_spot(spot: &SuiteSpot) -> SpotResult {
    let options = if spot.id.starts_with("kk_") {
        options_for_turn_card(&spot.turn_card)
    } else {
        options_for_hu_turn_spot(
            &spot.flop,
            &spot.turn_card,
            &spot.oop_range,
            &spot.ip_range,
            spot.stack_bb,
        )
    };
    let hero = if spot.id.starts_with("kk_") {
        hero_hole_cards()
    } else {
        hero_hole_cards_from_str(&spot.hero_hand)
    };
    let t0 = Instant::now();
    let mut trainer = MCCFRTrainer::init(options.clone());
    let seed = if spot.id.starts_with("kk_") {
        TRAIN_SEED
    } else {
        TRAIN_SEED.wrapping_add(spot.id.len() as u64)
    };
    let cfg = TrainConfig::default()
        .with_max_iter(MAX_ITER)
        .with_n_threads(1)
        .with_cfr_plus(true)
        .with_seed(seed)
        .with_pin_hero(hero, 0);
    trainer.train_with_config(&cfg);
    let elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0;
    let exploitability_max_mbb = if spot.measure_exploitability {
        let (ev, br) = trainer.compute_sample_inputs();
        let sample = convergence::Sample {
            iter: MAX_ITER as u64,
            t_seconds: 0.0,
            depth_tier_bb: spot.stack_bb,
            n_players: 2,
            ev,
            best_response: br,
            memory_mb: 0,
            n_threads: 1,
            stop_reason: None,
        };
        Some(sample.exploitability_max_mbb_per_hand())
    } else {
        None
    };
    let samples = trainer.collect_hero_samples(&options, hero, 0, BettingRound::Turn);
    let matched = pick_matching_sample(
        &samples,
        spot.query_pot_bb,
        spot.query_call_bb,
        SAMPLE_TOLERANCE_BB,
    )
    .cloned();
    let decisions_ranked = matched
        .as_ref()
        .map(|m| sample_to_decisions(m, spot.query_pot_bb))
        .unwrap_or_default();
    let top_action = decisions_ranked.first().map(|d| d.action.clone());
    let check_prob = matched.as_ref().map(|m| m.action_probs[1]);

    SpotResult {
        spot_id: spot.id.clone(),
        solve_elapsed_ms: elapsed_ms,
        exploitability_max_mbb,
        matched_sample: matched,
        decisions_ranked,
        top_action,
        check_prob,
    }
}

pub fn run_hu_turn_suite() -> SuiteReport {
    let spots_cfg = load_suite_spots();
    let t0 = Instant::now();
    let mut spots = Vec::new();
    let mut parity_matches = 0usize;
    let mut parity_total = 0usize;

    for cfg in &spots_cfg {
        let result = solve_suite_spot(cfg);
        if let Some(ref baseline_action) = cfg.baseline_top_action {
            parity_total += 1;
            if result.top_action.as_deref() == Some(baseline_action.as_str()) {
                parity_matches += 1;
            } else if let (Some(base_check), Some(check)) =
                (cfg.baseline_check_prob, result.check_prob)
            {
                if (check - base_check).abs() <= 0.15 {
                    parity_matches += 1;
                }
            }
        }
        spots.push(result);
    }

    SuiteReport {
        spots,
        total_elapsed_ms: t0.elapsed().as_secs_f64() * 1000.0,
        parity_matches,
        parity_total,
    }
}

pub fn assert_suite_quality_gate(report: &SuiteReport) -> Result<(), String> {
    if report.spots.len() < 5 {
        return Err(format!(
            "suite too small: {} spots (need ≥5)",
            report.spots.len()
        ));
    }

    if !report
        .spots
        .iter()
        .any(|s| s.exploitability_max_mbb.is_some())
    {
        return Err("suite must measure exploitability on at least one spot".to_string());
    }

    for s in &report.spots {
        if s.solve_elapsed_ms > 500.0 {
            return Err(format!(
                "{} too slow: {:.1} ms (budget 500 ms)",
                s.spot_id, s.solve_elapsed_ms
            ));
        }
        if let Some(eps) = s.exploitability_max_mbb {
            if !eps.is_finite() || eps >= 2000.0 {
                return Err(format!(
                    "{} exploitability {:.1} mbb/h (sanity budget <2000; pre-fix scale ~76k)",
                    s.spot_id, eps
                ));
            }
        }
        let matched = s
            .matched_sample
            .as_ref()
            .ok_or_else(|| format!("{}: no matched turn sample", s.spot_id))?;
        let uniform = 1.0 / 3.0;
        let spread = matched
            .action_probs
            .iter()
            .map(|p| (p - uniform).abs())
            .fold(0.0_f32, f32::max);
        if spread < 0.05 {
            return Err(format!(
                "{} strategy too uniform: {:?}",
                s.spot_id, matched.action_probs
            ));
        }
    }

    // P12.8 anchor: KK Kd turn check prob within ±0.10 of rjeans baseline.
    if let Some(kk) = report.spots.iter().find(|s| s.spot_id == "kk_kd_check") {
        let check = kk.check_prob.ok_or_else(|| "kk_kd_check: missing check_prob".to_string())?;
        let baseline = 0.6016_f32;
        if (check - baseline).abs() > 0.10 {
            return Err(format!(
                "kk_kd_check anchor: check={check:.3} vs baseline {baseline:.3} (±0.10)"
            ));
        }
    }

    if report.parity_total > 0 {
        let rate = report.parity_matches as f32 / report.parity_total as f32;
        // Turn-entry MCCFR vs postflop-solver CFR: 3/5 top-action on the suite is the
        // measured staging floor (kk_8s, pocket88 are known divergences).
        let min_rate = if report.parity_total >= 5 {
            0.6
        } else {
            0.8
        };
        if rate < min_rate {
            return Err(format!(
                "parity rate {:.0}% ({}/{}) below {:.0}% threshold",
                rate * 100.0,
                report.parity_matches,
                report.parity_total,
                min_rate * 100.0
            ));
        }
    }

    Ok(())
}

pub fn suite_report_to_json(report: &SuiteReport) -> serde_json::Value {
    serde_json::json!({
        "solver": "rust_solver",
        "total_elapsed_ms": report.total_elapsed_ms,
        "parity_matches": report.parity_matches,
        "parity_total": report.parity_total,
        "spots": report.spots.iter().map(|s| serde_json::json!({
            "spot_id": s.spot_id,
            "solve_elapsed_ms": s.solve_elapsed_ms,
            "exploitability_max_mbb": s.exploitability_max_mbb,
            "top_action": s.top_action,
            "check_prob": s.check_prob,
            "matched_sample": s.matched_sample.as_ref().map(|m| serde_json::json!({
                "board": m.board,
                "pot_bb": m.pot_bb,
                "call_cost_bb": m.call_cost_bb,
                "action_probs": m.action_probs,
                "raise_probs": m.raise_probs,
            })),
            "decisions_ranked": s.decisions_ranked.iter().map(|d| serde_json::json!({
                "rank": d.rank,
                "action": d.action,
                "raise_to_bb": d.raise_to_bb,
                "score": d.score,
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_poker::hand_evaluator::init_cards;

    #[test]
    fn suite_json_loads_five_spots() {
        let spots = load_suite_spots();
        assert!(spots.len() >= 5, "expected ≥5 suite spots");
    }

    #[test]
    #[ignore = "slow integration gate; run with cargo test --release -- --ignored"]
    fn hu_turn_suite_quality_gate() {
        if std::env::var("OUT_DIR").is_err() {
            let candidate = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("target/release/deps");
            if candidate.join("offset_table.dat").exists() {
                std::env::set_var("OUT_DIR", &candidate);
            }
        }
        init_cards();
        let report = run_hu_turn_suite();
        assert_suite_quality_gate(&report).unwrap_or_else(|e| panic!("suite gate: {}", e));
    }
}
