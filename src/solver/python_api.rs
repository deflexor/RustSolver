//! Python-facing solve API (`solver_ext`-compatible samples).

use std::time::Instant;

use rust_poker::constants::{RANK_TO_CHAR, SUIT_TO_CHAR};
use rust_poker::hand_range::{get_card_mask, HandRange, HoleCards};

use crate::cfr::{HeroDecisionSample, MCCFRTrainer, TrainConfig};
use crate::options::{solver_ext_action_abstraction, solver_ext_turn_action_abstraction, Options, HU_CX_FLOP_POT_CHIPS};
use crate::range_parse::expand_ppt_range;
use crate::state::BettingRound;

/// Default pot at turn decision when using legacy turn-entry mode (6.16 BB).
pub const DEFAULT_TURN_POT_CHIPS: u32 = 616;

/// One hero decision node in `solver_ext.TrainingSample` shape.
#[derive(Debug, Clone)]
pub struct TrainingSample {
    pub hero_hole: Vec<String>,
    pub board: Vec<String>,
    pub street: String,
    pub hero_pos: String,
    pub weip_flop: bool,
    pub pot_bb: f32,
    pub eff_stack_bb: f32,
    pub call_cost_bb: f32,
    pub min_raise_to_bb: f32,
    pub max_raise_to_bb: f32,
    pub action_probs: Vec<f32>,
    pub raise_probs: Vec<f32>,
    pub history_players: Vec<u8>,
    pub history_actions: Vec<String>,
    pub history_sizes: Vec<f32>,
}

impl TrainingSample {
    pub fn validate(&self) -> Result<(), String> {
        let action_sum: f32 = self.action_probs.iter().sum();
        if (action_sum - 1.0).abs() > 0.15 {
            return Err(format!("action_probs sum={action_sum:.3} (expected ~1.0)"));
        }
        if self.pot_bb <= 0.0 {
            return Err(format!("pot_bb={:.3} must be positive", self.pot_bb));
        }
        if self.eff_stack_bb <= 0.0 {
            return Err(format!(
                "eff_stack_bb={:.3} must be positive",
                self.eff_stack_bb
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct SolveFlopTreeConfig {
    pub hero_hand: String,
    pub stack_bb: u32,
    pub flop: String,
    pub weip_flop: bool,
    pub max_iter: usize,
    pub turn_card_limit: usize,
    /// When set, solve exactly these turn cards (ignores `turn_card_limit`).
    pub turn_cards: Option<Vec<String>>,
    pub oop_range: Option<String>,
    pub ip_range: Option<String>,
    /// Pot in chips at the turn decision node (`None` → 616 = 6.16 BB).
    pub turn_pot_chips: Option<u32>,
    pub n_threads: usize,
}

impl Default for SolveFlopTreeConfig {
    fn default() -> Self {
        SolveFlopTreeConfig {
            hero_hand: "AsAh".into(),
            stack_bb: 12,
            flop: "4dQcQd".into(),
            weip_flop: false,
            max_iter: 200,
            turn_card_limit: 2,
            turn_cards: None,
            oop_range: None,
            ip_range: None,
            turn_pot_chips: None,
            n_threads: 1,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SolveFlopTreeResult {
    pub elapsed_ms: f64,
    pub samples: Vec<TrainingSample>,
    pub turn_cards_sampled: Vec<String>,
}

pub fn parse_hero_hole(hand: &str) -> HoleCards {
    let mask = get_card_mask(hand);
    let c1 = mask.trailing_zeros() as u8;
    let c2 = (mask ^ (1u64 << c1)).trailing_zeros() as u8;
    HoleCards(c1, c2)
}

pub fn hero_hole_vec(hand: &str) -> Vec<String> {
    let mask = get_card_mask(hand);
    let mut cards = Vec::new();
    let mut m = mask;
    while m.count_ones() > 0 {
        let c = m.trailing_zeros() as u8;
        let rank = usize::from(c >> 2);
        let suit = usize::from(c & 3);
        cards.push(format!("{}{}", RANK_TO_CHAR[rank], SUIT_TO_CHAR[suit]));
        m ^= 1u64 << c;
    }
    cards
}

fn parse_range(s: &str) -> HandRange {
    HandRange::from_string(expand_ppt_range(s))
}

fn load_ranges(oop: Option<&str>, ip: Option<&str>) -> (HandRange, HandRange) {
    match (oop, ip) {
        (Some(o), Some(i)) if !o.is_empty() && !i.is_empty() => {
            (parse_range(o), parse_range(i))
        }
        _ => {
            let raw = include_str!("../../benchmarks/kk_turn_expanded_combos.txt");
            let mut lines = raw.lines().filter(|l| !l.is_empty());
            (
                HandRange::from_string(lines.next().unwrap().to_string()),
                HandRange::from_string(lines.next().unwrap().to_string()),
            )
        }
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

/// All turn cards not on the flop (for Python `list_turn_cards`).
pub fn list_turn_cards_public(flop: &str) -> Vec<String> {
    list_turn_cards(flop)
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
    let mut all = Vec::new();
    for r in ranks.chars() {
        for s in suits.chars() {
            all.push(format!("{r}{s}"));
        }
    }
    all.into_iter()
        .filter(|c| !flop_cards.contains(c))
        .collect()
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
    for i in (1..items.len()).rev() {
        let j = (rng.next_u64() as usize) % (i + 1);
        items.swap(i, j);
    }
}

pub fn options_for_flop_entry(
    flop: &str,
    stack_bb: u32,
    oop_range: &HandRange,
    ip_range: &HandRange,
    starting_pot_chips: u32,
) -> Options {
    Options {
        n_players: 2,
        hand_ranges: vec![oop_range.clone(), ip_range.clone()],
        stack_sizes: vec![stack_bb * 100, stack_bb * 100],
        board_mask: get_card_mask(flop),
        starting_pot: starting_pot_chips,
        all_in_threshold: 1.5,
        max_raises: 3,
        action_abstraction: solver_ext_turn_action_abstraction(),
        depth_tier_bb: stack_bb,
        postflop_pot_override: None,
        rake: None,
        max_action_sequences_per_street: 200,
        preflop_ranges: None,
    }
}

pub fn options_for_turn_card(
    turn_card: &str,
    flop: &str,
    stack_bb: u32,
    oop_range: &HandRange,
    ip_range: &HandRange,
    turn_pot_chips: u32,
) -> Options {
    let board = format!("{flop}{turn_card}");
    Options {
        n_players: 2,
        hand_ranges: vec![oop_range.clone(), ip_range.clone()],
        stack_sizes: vec![stack_bb * 100, stack_bb * 100],
        board_mask: get_card_mask(&board),
        starting_pot: turn_pot_chips,
        all_in_threshold: 1.5,
        max_raises: 3,
        action_abstraction: solver_ext_turn_action_abstraction(),
        depth_tier_bb: stack_bb,
        postflop_pot_override: None,
        rake: None,
        max_action_sequences_per_street: 200,
        preflop_ranges: None,
    }
}

fn hero_pos_label(weip_flop: bool) -> &'static str {
    if weip_flop {
        "IP"
    } else {
        "OOP"
    }
}

fn sample_to_training(
    hero_hand: &str,
    weip_flop: bool,
    stack_bb: u32,
    s: &HeroDecisionSample,
) -> TrainingSample {
    let eff = stack_bb as f32;
    TrainingSample {
        hero_hole: hero_hole_vec(hero_hand),
        board: s.board.clone(),
        street: s.street.clone(),
        hero_pos: hero_pos_label(weip_flop).to_string(),
        weip_flop,
        pot_bb: s.pot_bb,
        eff_stack_bb: eff,
        call_cost_bb: s.call_cost_bb,
        min_raise_to_bb: s.call_cost_bb.max(0.5),
        max_raise_to_bb: eff,
        action_probs: s.action_probs.to_vec(),
        raise_probs: s.raise_probs.to_vec(),
        history_players: Vec::new(),
        history_actions: Vec::new(),
        history_sizes: Vec::new(),
    }
}

/// `solver_ext.SolverSession.solve_flop_tree` equivalent (turn-entry per sampled card).
pub fn solve_flop_tree(cfg: &SolveFlopTreeConfig) -> SolveFlopTreeResult {
    let t0 = Instant::now();
    let hero = parse_hero_hole(&cfg.hero_hand);
    let hero_player = if cfg.weip_flop { 1 } else { 0 };
    let (oop_range, ip_range) = load_ranges(
        cfg.oop_range.as_deref(),
        cfg.ip_range.as_deref(),
    );
    let turn_pot = cfg
        .turn_pot_chips
        .unwrap_or(HU_CX_FLOP_POT_CHIPS);
    let turn_cards = if let Some(cards) = &cfg.turn_cards {
        cards.clone()
    } else {
        sample_turn_cards(&cfg.flop, &cfg.hero_hand, cfg.turn_card_limit)
    };

    let mut all_samples = Vec::new();
    for tc in &turn_cards {
        let options = options_for_turn_card(
            tc,
            &cfg.flop,
            cfg.stack_bb,
            &oop_range,
            &ip_range,
            turn_pot,
        );
/// Deterministic training seed (KK benchmark / production default).
pub const DEFAULT_TRAIN_SEED: u64 = 0x4b4b5f7475726e;

    let mut trainer = MCCFRTrainer::init(options.clone());
    let train_cfg = TrainConfig::default()
        .with_max_iter(cfg.max_iter)
        .with_n_threads(cfg.n_threads.max(1))
        .with_cfr_plus(true)
        .with_seed(DEFAULT_TRAIN_SEED)
        .with_pin_hero(hero, hero_player);
        trainer.train_with_config(&train_cfg);

        for street in [BettingRound::Turn, BettingRound::River] {
            let raw = trainer.collect_hero_samples(&options, hero, hero_player, street);
            for s in raw {
                let ts = sample_to_training(&cfg.hero_hand, cfg.weip_flop, cfg.stack_bb, &s);
                if ts.validate().is_ok() {
                    all_samples.push(ts);
                }
            }
        }
    }

    SolveFlopTreeResult {
        elapsed_ms: t0.elapsed().as_secs_f64() * 1000.0,
        samples: all_samples,
        turn_cards_sampled: turn_cards,
    }
}
