//! KK turn spot benchmark (table_3_20260708_040229).
//!
//! Mirrors rjeans `solve_flop_tree` inputs from `benchmarks/kk_turn_040229_prompt.md`.

use std::time::Instant;

use rust_poker::constants::{RANK_TO_CHAR, SUIT_TO_CHAR};
use rust_poker::hand_range::{get_card_mask, HandRange, HoleCards};

use crate::cfr::{HeroDecisionSample, MCCFRTrainer, TrainConfig};
use crate::options::Options;
use crate::range_parse::expand_ppt_range;
use crate::state::BettingRound;

pub const OOP_RANGE: &str =
    "66+,A8s+,A5s-A4s,AJo+,K9s+,KQo,QTs+,JTs,96s+,85s+,75s+,65s,54s";
pub const IP_RANGE: &str =
    "QQ-22,AQs-A2s,ATo+,K5s+,KJo+,Q8s+,J8s+,T7s+,96s+,86s+,75s+,64s+,53s+";

pub const HERO_HAND: &str = "KsKc";
pub const FLOP: &str = "4dQcQd";
pub const QUERY_POT_BB: f32 = 6.16;
pub const QUERY_CALL_BB: f32 = 0.0;
pub const STACK_BUCKET_BB: u32 = 12;
pub const MAX_ITER: usize = 200;
pub const TURN_CARD_LIMIT: usize = 2;
pub const SAMPLE_TOLERANCE_BB: f32 = 0.5;

const CHIPS_PER_BB: f32 = 100.0;
/// Pot at the turn decision node (matches TUI / rjeans baseline).
const TURN_ENTRY_POT_CHIPS: u32 = 616;

#[derive(Debug, Clone)]
pub struct RankedDecision {
    pub rank: usize,
    pub action: String,
    pub raise_to_bb: Option<f32>,
    pub score: f32,
}

#[derive(Debug, Clone)]
pub struct BaselineExpectation {
    pub solve_elapsed_ms: f32,
    pub total_samples: usize,
    pub top_action: String,
    pub top_score: f32,
    pub action_probs: [f32; 3],
    pub raise_probs: [f32; 4],
}

impl BaselineExpectation {
    pub fn rjeans_stack_12() -> Self {
        BaselineExpectation {
            solve_elapsed_ms: 634.5,
            total_samples: 2320,
            top_action: "call".into(),
            top_score: 0.489746,
            action_probs: [0.0, 0.489746, 0.510254],
            raise_probs: [0.009277, 0.009277, 0.009277, 0.972168],
        }
    }
}

#[derive(Debug, Clone)]
pub struct TurnSolveResult {
    pub turn_card: String,
    pub elapsed_ms: f64,
    pub samples: Vec<HeroDecisionSample>,
}

#[derive(Debug, Clone)]
pub struct BenchmarkReport {
    pub solver: String,
    pub stack_bucket_bb: u32,
    pub solve_elapsed_ms: f64,
    pub total_samples: usize,
    pub turn_boards_sampled: Vec<String>,
    pub matched_sample: Option<HeroDecisionSample>,
    pub decisions_ranked: Vec<RankedDecision>,
    pub exploitability_max_mbb: Option<f32>,
    pub baseline: BaselineExpectation,
}

/// Load OOP/IP combo lists. Prefer PPT expansion; fall back to the
/// postflop-solver expanded combo file when shorthand yields too few combos.
fn load_benchmark_ranges() -> (HandRange, HandRange) {
    let oop_ppt = HandRange::from_string(expand_ppt_range(OOP_RANGE));
    let ip_ppt = HandRange::from_string(expand_ppt_range(IP_RANGE));
    if oop_ppt.hands.len() > 100 && ip_ppt.hands.len() > 100 {
        return (oop_ppt, ip_ppt);
    }
    let raw = include_str!("../../../benchmarks/kk_turn_expanded_combos.txt");
    let mut lines = raw.lines().filter(|l| !l.is_empty());
    let oop_line = lines.next().expect("oop combos line");
    let ip_line = lines.next().expect("ip combos line");
    (
        HandRange::from_string(oop_line.to_string()),
        HandRange::from_string(ip_line.to_string()),
    )
}

/// Build `Options` for a turn-entry solve on one sampled turn card.
/// `starting_pot` matches the TUI decision-node geometry (6.16 BB).
pub fn options_for_turn_card(turn_card: &str) -> Options {
    let board = format!("{}{}", FLOP, turn_card);
    let (oop_range, ip_range) = load_benchmark_ranges();
    Options {
        n_players: 2,
        hand_ranges: vec![oop_range, ip_range],
        stack_sizes: vec![
            STACK_BUCKET_BB * 100,
            STACK_BUCKET_BB * 100,
        ],
        board_mask: get_card_mask(&board),
        starting_pot: TURN_ENTRY_POT_CHIPS,
        all_in_threshold: 1.5,
        max_raises: 3,
        action_abstraction: crate::actions::ActionAbstraction {
            bet_sizes: vec![vec![0.5, 0.75, 1.0], vec![0.5, 0.75, 1.0]],
            raise_sizes: vec![vec![2.5], vec![2.5]],
        },
        depth_tier_bb: STACK_BUCKET_BB,
        postflop_pot_override: None,
        rake: None,
        max_action_sequences_per_street: 200,
        preflop_ranges: None,
    }
}

/// Deterministic turn-card shuffle (matches `solver_ext` seed).
pub fn sample_turn_cards(flop: &str, hero_hand: &str, limit: usize) -> Vec<String> {
    let mut cards = list_turn_cards(flop);
    let seed = flop
        .bytes()
        .chain(hero_hand.bytes())
        .fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(u64::from(b)));
    fisher_yates_shuffle(&mut cards, seed);
    if limit > 0 && cards.len() > limit {
        cards.truncate(limit);
    }
    cards
}

fn list_turn_cards(flop: &str) -> Vec<String> {
    let flop_cards: Vec<String> = (0..flop.len())
        .step_by(2)
        .filter_map(|i| {
            if i + 2 <= flop.len() {
                Some(flop[i..i + 2].to_string())
            } else {
                None
            }
        })
        .collect();

    let ranks = "23456789TJQKA";
    let suits = "hdcs";
    let mut all_cards = Vec::new();
    for r in ranks.chars() {
        for s in suits.chars() {
            all_cards.push(format!("{r}{s}"));
        }
    }
    all_cards
        .into_iter()
        .filter(|c| !flop_cards.contains(c))
        .collect()
}

fn card_idx_to_str(card: u8) -> String {
    let rank = usize::from(card >> 2);
    let suit = usize::from(card & 3);
    format!("{}{}", RANK_TO_CHAR[rank], SUIT_TO_CHAR[suit])
}

struct SimpleRng {
    state: u64,
}

impl SimpleRng {
    fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 { 0xdead_beef } else { seed },
        }
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }
}

fn fisher_yates_shuffle(items: &mut [String], seed: u64) {
    let mut rng = SimpleRng::new(seed);
    let n = items.len();
    for i in (1..n).rev() {
        let j = (rng.next_u64() as usize) % (i + 1);
        items.swap(i, j);
    }
}

pub fn hero_hole_cards() -> HoleCards {
    let mask = get_card_mask(HERO_HAND);
    let c1 = mask.trailing_zeros() as u8;
    let c2 = (mask ^ (1u64 << c1)).trailing_zeros() as u8;
    HoleCards(c1, c2)
}

pub fn pick_matching_sample<'a>(
    samples: &'a [HeroDecisionSample],
    pot_bb: f32,
    call_bb: f32,
    tolerance_bb: f32,
) -> Option<&'a HeroDecisionSample> {
    let mut best: Option<(&HeroDecisionSample, f32)> = None;
    for s in samples.iter().filter(|s| s.street == "turn") {
        let dist = (s.pot_bb - pot_bb).abs() + (s.call_cost_bb - call_bb).abs();
        if dist <= tolerance_bb {
            match best {
                None => best = Some((s, dist)),
                Some((_, d)) if dist < d => best = Some((s, dist)),
                _ => {}
            }
        }
    }
    if let Some((s, _)) = best {
        return Some(s);
    }
    samples.iter().find(|s| s.street == "turn")
}

pub fn sample_to_decisions(
    sample: &HeroDecisionSample,
    query_pot_bb: f32,
) -> Vec<RankedDecision> {
    let (fold_p, call_p, raise_p) = (
        sample.action_probs[0],
        sample.action_probs[1],
        sample.action_probs[2],
    );
    let mut decisions: Vec<(String, Option<f32>, f32)> = Vec::new();

    if call_p > 0.001 {
        decisions.push(("call".into(), None, call_p));
    }
    if raise_p > 0.001 {
        let multipliers = [0.50_f32, 0.75, 1.00];
        for (i, mult) in multipliers.iter().enumerate() {
            if i < sample.raise_probs.len() {
                let target = mult * query_pot_bb;
                decisions.push((
                    "raise".into(),
                    Some(target),
                    raise_p * sample.raise_probs[i],
                ));
            }
        }
        let ai_idx = 3;
        if ai_idx < sample.raise_probs.len() {
            decisions.push((
                "raise".into(),
                Some(STACK_BUCKET_BB as f32),
                raise_p * sample.raise_probs[ai_idx],
            ));
        }
    }
    if sample.call_cost_bb > 0.0 && fold_p > 0.001 {
        decisions.push(("fold".into(), None, fold_p));
    }

    decisions.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
    decisions
        .into_iter()
        .take(5)
        .enumerate()
        .map(|(i, (action, raise_to_bb, score))| RankedDecision {
            rank: i + 1,
            action,
            raise_to_bb,
            score,
        })
        .collect()
}

pub fn solve_turn_card(turn_card: &str) -> TurnSolveResult {
    let options = options_for_turn_card(turn_card);
    let hero = hero_hole_cards();
    let t0 = Instant::now();
    let mut trainer = MCCFRTrainer::init(options.clone());
    let cfg = TrainConfig::default()
        .with_max_iter(MAX_ITER)
        .with_n_threads(1)
        .with_cfr_plus(false)
        .with_pin_hero(hero, 0);
    trainer.train_with_config(&cfg);
    let elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0;
    let samples = trainer.collect_hero_samples(&options, hero, 0, BettingRound::Turn);
    TurnSolveResult {
        turn_card: turn_card.to_string(),
        elapsed_ms,
        samples,
    }
}

/// Phase 10.7 quality gate checks on a benchmark report.
pub fn assert_quality_gate(report: &BenchmarkReport) -> Result<(), String> {
    let matched = report
        .matched_sample
        .as_ref()
        .ok_or_else(|| "no matched turn sample".to_string())?;

    let pot_err = (matched.pot_bb - QUERY_POT_BB).abs();
    let call_err = (matched.call_cost_bb - QUERY_CALL_BB).abs();
    if pot_err > SAMPLE_TOLERANCE_BB || call_err > SAMPLE_TOLERANCE_BB {
        return Err(format!(
            "geometry mismatch: pot_bb={:.3} (want {:.3}), call_cost_bb={:.3} (want {:.3})",
            matched.pot_bb, QUERY_POT_BB, matched.call_cost_bb, QUERY_CALL_BB
        ));
    }

    let uniform = 1.0 / 3.0;
    let spread = matched
        .action_probs
        .iter()
        .map(|p| (p - uniform).abs())
        .fold(0.0_f32, f32::max);
    if spread < 0.05 {
        return Err(format!(
            "strategy too uniform: action_probs={:?}",
            matched.action_probs
        ));
    }

    if report.solve_elapsed_ms > 500.0 {
        return Err(format!(
            "solve too slow: {:.1} ms (budget 500 ms)",
            report.solve_elapsed_ms
        ));
    }

    // Exploitability gate (P10.7 stretch) — skipped until BR scale is trustworthy
    // on turn-entry trees; `exploitability_max_mbb` is optional on the report.

    Ok(())
}

pub fn run_kk_turn_benchmark() -> BenchmarkReport {
    let turn_cards = sample_turn_cards(FLOP, HERO_HAND, TURN_CARD_LIMIT);
    let t0 = Instant::now();
    let mut all_samples = Vec::new();
    let mut boards_sampled = Vec::new();

    for tc in &turn_cards {
        let result = solve_turn_card(tc);
        boards_sampled.push(tc.clone());
        all_samples.extend(result.samples);
    }

    let solve_elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0;
    let matched = pick_matching_sample(
        &all_samples,
        QUERY_POT_BB,
        QUERY_CALL_BB,
        SAMPLE_TOLERANCE_BB,
    )
    .cloned();
    let decisions_ranked = matched
        .as_ref()
        .map(|m| sample_to_decisions(m, QUERY_POT_BB))
        .unwrap_or_default();
    let baseline = BaselineExpectation::rjeans_stack_12();

    BenchmarkReport {
        solver: "rust_solver".into(),
        stack_bucket_bb: STACK_BUCKET_BB,
        solve_elapsed_ms,
        total_samples: all_samples.len(),
        turn_boards_sampled: boards_sampled,
        matched_sample: matched,
        decisions_ranked,
        exploitability_max_mbb: None,
        baseline,
    }
}

pub fn print_report(report: &BenchmarkReport) {
    println!("=== KK turn benchmark (rust_solver) ===");
    println!("stack_bucket_bb: {}", report.stack_bucket_bb);
    println!("solve_elapsed_ms: {:.2}", report.solve_elapsed_ms);
    println!("total_samples: {}", report.total_samples);
    println!("turn_boards_sampled: {:?}", report.turn_boards_sampled);

    if let Some(eps) = report.exploitability_max_mbb {
        println!("exploitability_max_mbb: {:.2}", eps);
    }

    if let Some(m) = &report.matched_sample {
        println!("matched_sample:");
        println!("  board: {:?}", m.board);
        println!("  pot_bb: {:.4}", m.pot_bb);
        println!("  call_cost_bb: {:.4}", m.call_cost_bb);
        println!(
            "  action_probs: [{:.6}, {:.6}, {:.6}]",
            m.action_probs[0], m.action_probs[1], m.action_probs[2]
        );
        println!(
            "  raise_probs: [{:.6}, {:.6}, {:.6}, {:.6}]",
            m.raise_probs[0], m.raise_probs[1], m.raise_probs[2], m.raise_probs[3]
        );
    } else {
        println!("matched_sample: none");
    }

    println!("decisions_ranked:");
    for d in &report.decisions_ranked {
        match d.raise_to_bb {
            Some(rt) => println!(
                "  {}. {} raise_to={:.2} score={:.6}",
                d.rank, d.action, rt, d.score
            ),
            None => println!("  {}. {} score={:.6}", d.rank, d.action, d.score),
        }
    }

    println!();
    println!("=== vs rjeans baseline (stack=12) ===");
    let b = &report.baseline;
    println!("baseline solve_elapsed_ms: {:.1}", b.solve_elapsed_ms);
    println!(
        "baseline action_probs: [{:.6}, {:.6}, {:.6}]",
        b.action_probs[0], b.action_probs[1], b.action_probs[2]
    );
    println!(
        "baseline top: {} @ {:.6}",
        b.top_action, b.top_score
    );

    if let Some(top) = report.decisions_ranked.first() {
        let speed_ratio = report.solve_elapsed_ms / f64::from(b.solve_elapsed_ms);
        println!(
            "rust_solver top: {} @ {:.6} ({:.2}x baseline time)",
            top.action, top.score, speed_ratio
        );
        let check_delta = top.score - b.top_score;
        println!("top-action score delta: {:+.6}", check_delta);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_poker::hand_evaluator::init_cards;

    #[test]
    fn turn_card_sampling_matches_rjeans() {
        let cards = sample_turn_cards(FLOP, HERO_HAND, TURN_CARD_LIMIT);
        assert_eq!(cards, vec!["Kd", "8s"]);
    }

    #[test]
    fn benchmark_ranges_parse_and_tree_builds() {
        if std::env::var("OUT_DIR").is_err() {
            let candidate = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("target/release/deps");
            if candidate.join("offset_table.dat").exists() {
                std::env::set_var("OUT_DIR", &candidate);
            }
        }
        init_cards();
        let options = options_for_turn_card("Kd");
        eprintln!("oop combos: {}", options.hand_ranges[0].hands.len());
        eprintln!("ip combos: {}", options.hand_ranges[1].hands.len());
        let (n_actions, _tree) = crate::tree_builder::build_game_tree(&options);
        eprintln!("tree action nodes: {}", n_actions);
        assert!(options.hand_ranges[0].hands.len() > 100);
        assert!(options.hand_ranges[1].hands.len() > 100);
        assert_eq!(options.starting_pot, TURN_ENTRY_POT_CHIPS);
        assert!(n_actions > 0 && n_actions < 10_000);
    }

    /// Phase 10.7: KK turn quality gate (non-uniform strategy, geometry, speed).
    #[test]
    #[ignore = "slow integration gate; run with cargo test --release -- --ignored"]
    fn kk_turn_quality_gate() {
        if std::env::var("OUT_DIR").is_err() {
            let candidate = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("target/release/deps");
            if candidate.join("offset_table.dat").exists() {
                std::env::set_var("OUT_DIR", &candidate);
            }
        }
        init_cards();
        let report = run_kk_turn_benchmark();
        assert_quality_gate(&report).unwrap_or_else(|e| panic!("quality gate: {}", e));
    }
}
