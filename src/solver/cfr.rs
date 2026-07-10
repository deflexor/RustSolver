use std::sync::atomic::{AtomicBool, Ordering};
use std::{thread, time};
use crossbeam::atomic::AtomicCell;
use std::sync::Arc;
use std::sync::Mutex;

use rand::{SeedableRng, thread_rng};
use rand::Rng;
use rand::rngs::SmallRng;
use rand::seq::SliceRandom;
use rand::distributions::WeightedIndex;
use rand::distributions::{Distribution, Uniform};

use rust_poker::hand_range::{HandRange, HoleCards};
use rust_poker::equity_calculator::{get_board_from_bit_mask, calc_equity, remove_invalid_combos};
use rust_poker::constants::{CARD_COUNT, RANK_TO_CHAR, SUIT_TO_CHAR};
use rust_poker::hand_evaluator::{Hand, CARDS, evaluate};

use rayon::prelude::*;

use crate::actions::Action;
use crate::tree::{Tree, NodeId};
use crate::nodes::{PublicChanceNode, TerminalType, TerminalNode};
use crate::tree_builder::build_game_tree;
use crate::infoset::{Infoset, InfosetTable, create_infosets};
use crate::nodes::GameTreeNode;
use crate::options::Options;
use crate::card_abstraction::{CardAbstraction, ISOMORPHIC, EMD, ICardAbstraction};
use crate::state::{BettingRound, GameState};

#[derive(Debug, Clone)]
struct TrainHand {
    pub hands: Vec<HoleCards>,
    pub board: [u8; 7],
}

impl TrainHand {
    /// Returns eval repr of the player's 5- or 7-card hand.
    fn get_hand(&self, player: u8) -> Hand {
        let mut hand = Hand::empty();
        let cards = CARDS.get().expect("rust_poker CARDS not initialized");
        let p = usize::from(player);
        if p < self.hands.len() {
            hand += cards[usize::from(self.hands[p].0)];
            hand += cards[usize::from(self.hands[p].1)];
        }
        for i in 2..7 {
            hand += cards[usize::from(self.board[i])]
        }
        return hand;
    }

    fn num_players(&self) -> usize {
        self.hands.len()
    }
}

fn generate_possible_next_deals(round: BettingRound, hand: &TrainHand) -> Vec<u8> {
    let mut used_cards_mask = 0u64;
    let n_board_cards = match round {
        BettingRound::Flop => panic!("invalid number of board cards"),
        BettingRound::Turn => 3,
        BettingRound::River => 4,
    };
    for h in &hand.hands {
        used_cards_mask |= (1u64 << h.0) | (1u64 << h.1);
    }
    for i in 0..n_board_cards {
        used_cards_mask |= 1u64 << hand.board[i + 2];
    }
    let mut possible_cards: Vec<u8> = Vec::new();
    for i in 0..CARD_COUNT {
        if (1u64 << i) & used_cards_mask == 0 {
            possible_cards.push(i);
        }
    }
    return possible_cards;
}

/// get all possible hole card combos
fn generate_all_hole_card_combos(mut board_mask: u64, hand_ranges: &[HandRange]) -> Vec<TrainHand> {
    let mut board = [0u8; 7];
    let mut i = 2;
    while board_mask.count_ones() > 0 {
        board[i] = board_mask.trailing_zeros() as u8;
        board_mask ^= 1u64 << board_mask.trailing_zeros();
        i += 1;
    }

    let mut combos: Vec<TrainHand> = Vec::new();
    let n = hand_ranges.len();
    // Recursive enumeration: pick one combo from each player's range
    // such that no two combos share a card.
    fn recurse(
        idx: usize,
        ranges: &[HandRange],
        used: u64,
        current: &mut Vec<HoleCards>,
        out: &mut Vec<TrainHand>,
        board: &[u8; 7],
    ) {
        if idx == ranges.len() {
            out.push(TrainHand {
                board: *board,
                hands: current.clone(),
            });
            return;
        }
        for c in &ranges[idx].hands {
            let mask = (1u64 << c.0) | (1u64 << c.1);
            if mask & used == 0 {
                current.push(*c);
                recurse(idx + 1, ranges, used | mask, current, out, board);
                current.pop();
            }
        }
    }
    let mut current: Vec<HoleCards> = Vec::with_capacity(n);
    recurse(0, hand_ranges, 0, &mut current, &mut combos, &board);
    return combos;
}

fn hand_conflicts(hand: HoleCards, board_mask: u64) -> bool {
    let mask = (1u64 << hand.0) | (1u64 << hand.1);
    mask & board_mask != 0
}

fn generate_hand<R: Rng>(
    rng: &mut R,
    mut board_mask: u64,
    hand_ranges: &[HandRange],
    pin_hero: Option<(HoleCards, u8)>,
) -> TrainHand {
    let mut used_cards_mask = board_mask;
    let mut board = [0u8; 7];
    let mut i = 2;

    while board_mask.count_ones() > 0 {
        board[i] = board_mask.trailing_zeros() as u8;
        board_mask ^= 1u64 << board_mask.trailing_zeros();
        i += 1;
    }
    // Future street cards are dealt at PublicChance nodes during traversal.

    let mut hands: Vec<HoleCards> = Vec::with_capacity(hand_ranges.len());
    for (p, range) in hand_ranges.iter().enumerate() {
        if let Some((hero, hero_p)) = pin_hero {
            if p == usize::from(hero_p) {
                hands.push(hero);
                used_cards_mask |= (1u64 << hero.0) | (1u64 << hero.1);
                continue;
            }
        }
        let valid: Vec<HoleCards> = range
            .hands
            .iter()
            .copied()
            .filter(|c| {
                let combo_mask = (1u64 << c.0) | (1u64 << c.1);
                combo_mask & used_cards_mask == 0
            })
            .collect();
        let Some(c) = valid.choose(rng) else {
            // No legal combo for this player; caller will get a partial hand.
            return TrainHand {
                board,
                hands,
            };
        };
        let combo_mask = (1u64 << c.0) | (1u64 << c.1);
        used_cards_mask |= combo_mask;
        hands.push(*c);
    }

    TrainHand { board, hands }
}


/**
 * A structure to implement monte carlo cfr
 */
#[derive(Debug)]
pub struct MCCFRTrainer {
    infosets: InfosetTable,
    game_tree: Tree<GameTreeNode>,
    card_abs: Vec<CardAbstraction>,
    hand_ranges: Vec<HandRange>,
    initial_board_mask: u64,
    /// Depth tier (in BB) the trainer was constructed for. Used by
    /// the convergence writer to label `Sample::depth_tier_bb`.
    pub depth_tier_bb: u32,
}

/// Training configuration. Constructed with `TrainConfig::default()`
/// and tweaked via the `with_*` builders. Passed to
/// `MCCFRTrainer::train_with_config`.
#[derive(Debug, Clone)]
pub struct TrainConfig {
    pub max_iter: usize,
    pub target_exploitability_mbb: Option<f32>,
    pub convergence_interval: usize,
    pub convergence_path: Option<std::path::PathBuf>,
    /// When `true`, use CFR+ (Brown & Sandholm 2019): regret floor at
    /// 0 and strategy_sum weighted by iteration count. Substantially
    /// faster convergence on 2p river subgames.
    pub cfr_plus: bool,
    /// Rayon worker count. `None` uses `available_parallelism()` (P9.3).
    pub n_threads: Option<usize>,
    /// Stop training after this wall-clock budget (P10.4).
    pub time_budget_ms: Option<u64>,
    /// Pin hero hole cards every iteration for query-spot training (P10.3).
    pub pin_hero: Option<(HoleCards, u8)>,
    /// RNG seed for reproducible training (`None` = thread_rng per iter).
    pub seed: Option<u64>,
}

impl Default for TrainConfig {
    fn default() -> Self {
        TrainConfig {
            max_iter: 10_000_000,
            target_exploitability_mbb: None,
            convergence_interval: 100_000,
            convergence_path: None,
            cfr_plus: false,
            n_threads: None,
            time_budget_ms: None,
            pin_hero: None,
            seed: None,
        }
    }
}

impl TrainConfig {
    pub fn with_max_iter(mut self, n: usize) -> Self {
        self.max_iter = n;
        self
    }
    pub fn with_target_exploitability_mbb(mut self, mbb: f32) -> Self {
        self.target_exploitability_mbb = Some(mbb);
        self
    }
    pub fn with_convergence_interval(mut self, n: usize) -> Self {
        self.convergence_interval = n;
        self
    }
    pub fn with_convergence_path<P: Into<std::path::PathBuf>>(mut self, p: P) -> Self {
        self.convergence_path = Some(p.into());
        self
    }
    pub fn with_cfr_plus(mut self, on: bool) -> Self {
        self.cfr_plus = on;
        self
    }
    pub fn with_n_threads(mut self, n: usize) -> Self {
        self.n_threads = Some(n);
        self
    }
    pub fn with_time_budget_ms(mut self, ms: u64) -> Self {
        self.time_budget_ms = Some(ms);
        self
    }
    pub fn with_pin_hero(mut self, hero: HoleCards, player: u8) -> Self {
        self.pin_hero = Some((hero, player));
        self
    }
    pub fn with_seed(mut self, seed: u64) -> Self {
        self.seed = Some(seed);
        self
    }
}

fn estimate_rss_mb() -> u64 {
    #[cfg(target_os = "linux")]
    {
        if let Ok(s) = std::fs::read_to_string("/proc/self/statm") {
            // Format: <size> <resident> <shared> <text> <lib> <data> <dt>
            // Pages.
            if let Some(rss_pages) = s.split_whitespace().nth(1) {
                if let Ok(pages) = rss_pages.parse::<u64>() {
                    return pages * 4096 / (1024 * 1024);
                }
            }
        }
    }
    0
}

/// Build one card abstraction per betting round in the game tree.
/// Turn entry (4 board cards) yields `[Turn, River]`; flop entry
/// yields `[Flop, Turn, River]`. Skips unused streets so we do not
/// allocate flop isomorphism buckets when solving turn-only.
fn build_card_abstractions(
    hand_ranges: &Vec<HandRange>,
    board_mask: u64,
) -> Vec<CardAbstraction> {
    let board_count = board_mask.count_ones();
    let streets: Vec<BettingRound> = match board_count {
        3 => vec![BettingRound::Flop, BettingRound::Turn, BettingRound::River],
        4 => vec![BettingRound::Turn, BettingRound::River],
        5 => vec![BettingRound::River],
        n => panic!(
            "invalid board mask ({} cards); expected 3, 4, or 5",
            n
        ),
    };

    streets
        .into_iter()
        .map(|round| {
            let emd_path = match round {
                BettingRound::Flop => "round_1_emd.dat",
                BettingRound::Turn => "round_2_emd.dat",
                BettingRound::River => "round_3_emd.dat",
            };
            let use_emd = match round {
                BettingRound::Flop => board_count == 3
                    && std::path::Path::new(emd_path).exists(),
                BettingRound::Turn => board_count == 4
                    && std::path::Path::new(emd_path).exists(),
                BettingRound::River => false,
            };
            if use_emd {
                CardAbstraction::EMD(EMD::init(hand_ranges, board_mask, round))
            } else {
                CardAbstraction::ISOMORPHIC(ISOMORPHIC::init(
                    hand_ranges,
                    board_mask,
                    round,
                ))
            }
        })
        .collect()
}

impl MCCFRTrainer {
    pub fn init(options: Options) -> Self {

        let mut hand_ranges = options.hand_ranges.to_owned();

        remove_invalid_combos(&mut hand_ranges, options.board_mask);

        let (n_actions, game_tree) = build_game_tree(&options);

        let card_abs = build_card_abstractions(&hand_ranges, options.board_mask);

        // intialize infosets
        let infosets = create_infosets(n_actions, &game_tree, &card_abs);

        MCCFRTrainer {
            infosets,
            game_tree,
            hand_ranges,
            initial_board_mask: options.board_mask,
            card_abs,
            depth_tier_bb: options.depth_tier_bb,
        }
    }
    /**
     * iterations: number of iterations to train for
     */
    pub fn train(&mut self, iterations: usize) {
        let cfg = TrainConfig::default().with_max_iter(iterations);
        self.train_with_config(&cfg);
    }

    /// One external-sampling MCCFR iteration (Lanctot et al. 2009, P9.5).
    ///
    /// A single hand is drawn, one traverser is selected (rotating by
    /// `iter_idx`), and the tree is walked once. At the traverser's
    /// infosets all actions are evaluated and regrets are updated; at
    /// opponent infosets one action is sampled from their strategy.
    /// This replaces the prior pattern of one full traversal per player
    /// per iteration (~`n_players`× less tree work per iteration).
    fn run_one_iteration<R: Rng>(
        &self,
        rng: &mut R,
        iter_idx: usize,
        cfr_plus: bool,
        pin_hero: Option<(HoleCards, u8)>,
    ) {
        const PRUNE_THRESHOLD: usize = 10_000_000;
        let hand = generate_hand(
            rng,
            self.initial_board_mask,
            &self.hand_ranges,
            pin_hero,
        );
        if hand.hands.len() != self.hand_ranges.len() {
            return;
        }
        let q: f32 = rng.gen();
        let prune = iter_idx > PRUNE_THRESHOLD && q > 0.05;
        let n_players = self.hand_ranges.len();
        let traverser = (iter_idx % n_players) as u8;
        self.external_sampling_mccfr(
            rng,
            0,
            traverser,
            hand,
            1f32,
            prune,
            cfr_plus,
        );
    }

    /// Run MCCFR training with the given config. `cfg` controls
    /// convergence sampling, target exploitability, and the
    /// `convergence.jsonl` output path.
    pub fn train_with_config(&mut self, cfg: &TrainConfig) {
        const DISCOUNT_INTERVAL: usize = 100_000;
        const DISCOUNT_CAP: usize = 20_000_000;
        /// Rayon batch size per scheduling round (P9.3).
        const BATCH_MULTIPLIER: usize = 4;

        let max_iter = cfg.max_iter;
        let target_mbb = cfg.target_exploitability_mbb;
        let conv_interval = cfg.convergence_interval.max(1);
        let n_threads = cfg.n_threads.unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(8)
        });

        let n_players = self.hand_ranges.len();

        let t = Arc::new(AtomicCell::new(0usize));
        let stop = Arc::new(AtomicBool::new(false));
        let a_self = Arc::new(self);
        let started = std::time::Instant::now();
        let recorder = cfg
            .convergence_path
            .as_ref()
            .map(|p| convergence::Recorder::new(p));

        let cfr_plus = cfg.cfr_plus;
        let pin_hero = cfg.pin_hero;
        let time_budget_ms = cfg.time_budget_ms;
        let train_seed = cfg.seed;
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(n_threads)
            .build()
            .expect("rayon thread pool");

        crossbeam::scope(|scope| {
            // Discount thread (every DISCOUNT_INTERVAL iters).
            let a_self_discount = Arc::clone(&a_self);
            let t_discount = Arc::clone(&t);
            let stop_discount = Arc::clone(&stop);
            scope.spawn(move |_| {
                let mut threshold = DISCOUNT_INTERVAL;
                while !stop_discount.load(Ordering::Relaxed)
                    && t_discount.load() < max_iter
                {
                    thread::sleep(time::Duration::from_millis(1));
                    let tc = t_discount.load();
                    if tc > DISCOUNT_CAP {
                        break;
                    }
                    if tc > threshold {
                        let p = (tc / DISCOUNT_INTERVAL) as f32;
                        let d = p / (p + 1.0);
                        for row in a_self_discount.infosets.iter() {
                            for infoset in row.initialized_infosets() {
                                infoset.discount(d);
                            }
                        }
                        threshold = t_discount.load() + DISCOUNT_INTERVAL;
                    }
                }
            });

            // Convergence thread (every `conv_interval` iters).
            if let Some(recorder) = recorder {
                let a_self_conv = Arc::clone(&a_self);
                let t_conv = Arc::clone(&t);
                let stop_conv = Arc::clone(&stop);
                let depth_tier_bb = a_self_conv.depth_tier_bb;
                scope.spawn(move |_| {
                    let mut next_sample_at: u64 = conv_interval as u64;
                    let mut stop_reason: Option<String> = None;
                    while !stop_conv.load(Ordering::Relaxed) && t_conv.load() < max_iter {
                        thread::sleep(time::Duration::from_millis(1));
                        let tc = t_conv.load() as u64;
                        if tc < next_sample_at {
                            continue;
                        }
                        let (ev, br) = a_self_conv.compute_sample_inputs();
                        let sample = convergence::Sample {
                            iter: tc,
                            t_seconds: started.elapsed().as_secs_f64(),
                            depth_tier_bb,
                            n_players,
                            ev,
                            best_response: br,
                            memory_mb: a_self_conv.infosets.total_bytes() as u64 / (1024 * 1024),
                            n_threads,
                            stop_reason: None,
                        };
                        if let Some(target) = target_mbb {
                            if sample.exploitability_max_mbb_per_hand() <= target {
                                stop_reason = Some("target_reached".to_string());
                                stop_conv.store(true, Ordering::Relaxed);
                                if let Err(e) = recorder.write(&sample) {
                                    eprintln!("[convergence] write error: {}", e);
                                }
                                break;
                            }
                        }
                        if let Err(e) = recorder.write(&sample) {
                            eprintln!("[convergence] write error: {}", e);
                        }
                        next_sample_at = tc + conv_interval as u64;
                    }
                    if stop_reason.is_some() {
                        let (ev, br) = a_self_conv.compute_sample_inputs();
                        let final_sample = convergence::Sample {
                            iter: t_conv.load() as u64,
                            t_seconds: started.elapsed().as_secs_f64(),
                            depth_tier_bb,
                            n_players,
                            ev,
                            best_response: br,
                            memory_mb: a_self_conv.infosets.total_bytes() as u64 / (1024 * 1024),
                            n_threads,
                            stop_reason,
                        };
                        let _ = recorder.write(&final_sample);
                    }
                });
            }

            // P9.3: rayon parallel iteration batches.
            let stop_train = Arc::clone(&stop);
            let t_train = Arc::clone(&t);
            scope.spawn(move |_| {
                pool.install(|| {
                    let batch_size = (n_threads * BATCH_MULTIPLIER).max(1);
                    while !stop_train.load(Ordering::Relaxed) {
                        if let Some(budget) = time_budget_ms {
                            if started.elapsed().as_millis() as u64 >= budget {
                                stop_train.store(true, Ordering::Relaxed);
                                break;
                            }
                        }
                        let batch_start = t_train.fetch_add(batch_size);
                        if batch_start >= max_iter {
                            break;
                        }
                        let batch_end = (batch_start + batch_size).min(max_iter);
                        (batch_start..batch_end).into_par_iter().for_each(|iter_idx| {
                            if stop_train.load(Ordering::Relaxed) {
                                return;
                            }
                            let mut rng = match train_seed {
                                Some(s) => SmallRng::seed_from_u64(
                                    s.wrapping_add(iter_idx as u64),
                                ),
                                None => SmallRng::from_rng(thread_rng()).unwrap(),
                            };
                            a_self.run_one_iteration(&mut rng, iter_idx, cfr_plus, pin_hero);
                        });
                    }
                });
            });
        })
        .unwrap();
    }

    /// External-sampling MCCFR traversal for a single traverser.
    ///
    /// `traverser` is the only player whose regrets/strategy_sum are
    /// updated. Opponent nodes sample one action; traverser nodes
    /// enumerate all actions. `cfr_reach` tracks the product of
    /// sampled opponent reach probabilities.
    fn external_sampling_mccfr<R: Rng>(
        &self,
        rng: &mut R,
        node_id: NodeId,
        traverser: u8,
        mut hand: TrainHand,
        cfr_reach: f32,
        prune: bool,
        cfr_plus: bool,
    ) -> f32 {

        let node = self.game_tree.get_node(node_id);
        match &node.data {
            GameTreeNode::PublicChance(pc) => {
                let possible = generate_possible_next_deals(pc.round, &hand);
                if possible.is_empty() {
                    return 0.0;
                }
                let card = possible[rng.gen_range(0, possible.len())];
                let comm = (2..7).filter(|&i| hand.board[i] != 0).count();
                if comm >= 5 {
                    return 0.0;
                }
                let mut new_hand = hand;
                new_hand.board[2 + comm] = card;
                self.external_sampling_mccfr(
                    rng,
                    node.children[0],
                    traverser,
                    new_hand,
                    cfr_reach,
                    prune,
                    cfr_plus,
                )
            }
            GameTreeNode::PrivateChance => self.external_sampling_mccfr(
                rng,
                node.children[0],
                traverser,
                hand,
                cfr_reach,
                prune,
                cfr_plus,
            ),
            GameTreeNode::Terminal(tn) => {
                let my_wager = tn.player_wagers.get(usize::from(traverser)).copied().unwrap_or(0);
                match tn.ttype {
                    TerminalType::UNCONTESTED => {
                        if traverser == tn.last_to_act {
                            return 1.0 * ((tn.value as f32) - (my_wager as f32));
                        } else {
                            return -1.0 * (my_wager as f32);
                        }
                    }
                    TerminalType::SHOWDOWN | TerminalType::ALLIN => {
                        let n = hand.num_players();
                        if n < 2 {
                            return 0.0;
                        }
                        let scores: Vec<u16> =
                            (0..n).map(|p| evaluate(&hand.get_hand(p as u8))).collect();
                        let my_score = scores[usize::from(traverser)];
                        let any_higher = scores.iter().any(|&s| s > my_score);
                        if !any_higher {
                            return 1.0 * ((tn.value as f32) - (my_wager as f32));
                        } else {
                            return -1.0 * (my_wager as f32);
                        }
                    }
                }
            }
            GameTreeNode::Action(an) => {

                const PRUNE_THRESHOLD: i32 = -10000000;

                // get number of actions
                let n_actions = an.actions.len();

                // copy hole cards to board
                hand.board[0] = hand.hands[usize::from(an.player)].0;
                hand.board[1] = hand.hands[usize::from(an.player)].1;

                let cluster_idx = match &self.card_abs[usize::from(an.round_idx)] {
                    CardAbstraction::EMD(card_abs) => card_abs.get_cluster(&hand.board, an.player),
                    CardAbstraction::ISOMORPHIC(card_abs) => card_abs.get_cluster(&hand.board, an.player),
                    CardAbstraction::OCHS(card_abs) => card_abs.get_cluster(&hand.board, an.player)
                };

                {

                }
                if an.player == traverser {
                    let mut util = 0f32;
                    let mut utils = vec![0f32; n_actions];
                    let mut explored = vec![false; n_actions];

                    let infoset = self.infosets[an.index].get_or_init(cluster_idx);
                    let strategy = infoset.get_strategy();

                    for i in 0..n_actions {
                        if prune {
                            if infoset.regret(i) > PRUNE_THRESHOLD {
                                utils[i] = self.external_sampling_mccfr(
                                    rng,
                                    node.children[i],
                                    traverser,
                                    hand.clone(),
                                    cfr_reach,
                                    prune,
                                    cfr_plus,
                                );
                                util += utils[i] * strategy[i];
                                explored[i] = true;
                            }
                        } else {
                            utils[i] = self.external_sampling_mccfr(
                                rng,
                                node.children[i],
                                traverser,
                                hand.clone(),
                                cfr_reach,
                                prune,
                                cfr_plus,
                            );
                            util += utils[i] * strategy[i];
                        }
                    }

                    // Update regrets and strategy_sum through the
                    // atomic API. `add_regret` and `add_strategy_sum`
                    // saturate at i32::MIN/MAX internally. When
                    // `cfr_plus` is on, the regret floor is enforced
                    // here (after the update). The exact CFR+ strategy
                    // weighting (multiply by t) is approximate here:
                    // we add the strategy without the t multiplier
                    // and rely on the `floor_at_zero` change as the
                    // primary CFR+ benefit. A future revision can
                    // thread the iteration counter through `mccfr` to
                    // apply the exact weighting.
                    for i in 0..n_actions {
                        if prune && !explored[i] {
                            continue;
                        }
                        let regret_delta = (100.0 * cfr_reach * (utils[i] - util)) as i32;
                        infoset.add_regret(i, regret_delta);
                        let ssum_delta = (100.0 * cfr_reach * strategy[i]) as i32;
                        infoset.add_strategy_sum(i, ssum_delta);
                    }
                    if cfr_plus {
                        infoset.floor_regrets_at_zero();
                    }

                    return util;
                }
                // Opponent node: sample one action (external sampling).
                let row = &self.infosets[an.index];
                let strategy = row.strategy_or_uniform(cluster_idx);
                let dist = WeightedIndex::new(&strategy).unwrap();
                let a_idx = dist.sample(rng);
                return self.external_sampling_mccfr(
                    rng,
                    node.children[a_idx],
                    traverser,
                    hand,
                    cfr_reach * strategy[a_idx],
                    prune,
                    cfr_plus,
                );
            }
        }
    }

    // Non-sampling CFR (cfr) was removed in P0.4. It was dead code (the
    // external call site at line 217 was commented out) and contained an
    // unsafe raw-pointer cast into Infoset. A clean non-sampling CFR
    // implementation will be reintroduced as a separate `FullCFR` module
    // for the safe-search work in Phase 6.

    fn calc_br(&self) -> Vec<f32> {
        let op = self.initial_reach();
        let res = self.abstract_br(0, op, self.board_from_mask());
        let n_players = self.hand_ranges.len();
        let mut out = vec![0f32; n_players];
        for p in 0..n_players {
            for b in 0..res[p].len() {
                out[p] += res[p][b];
            }
            if !res[p].is_empty() {
                out[p] /= res[p].len() as f32;
            }
        }
        return out;
    }

    /// Average EV per player when all players use their average
    /// strategy (the same input the BR walker uses, but the actor at
    /// each infoset follows the average strategy instead of picking
    /// the best response). Returns a length-`n_players` vector.
    fn calc_ev(&self) -> Vec<f32> {
        let op = self.initial_reach();
        let res = self.abstract_ev(0, op, self.board_from_mask());
        let n_players = self.hand_ranges.len();
        let mut out = vec![0f32; n_players];
        for p in 0..n_players {
            for b in 0..res[p].len() {
                out[p] += res[p][b];
            }
            if !res[p].is_empty() {
                out[p] /= res[p].len() as f32;
            }
        }
        return out;
    }

    fn abs_size(&self, round_idx: u8, player: u8) -> usize {
        match &self.card_abs[usize::from(round_idx)] {
            CardAbstraction::ISOMORPHIC(a) => a.get_size(player),
            CardAbstraction::EMD(a) => a.get_size(player),
            CardAbstraction::OCHS(a) => a.get_size(player),
        }
    }

    fn get_cluster(&self, round_idx: u8, player: u8, board: &[u8; 7]) -> usize {
        match &self.card_abs[usize::from(round_idx)] {
            CardAbstraction::ISOMORPHIC(a) => a.get_cluster(board, player),
            CardAbstraction::EMD(a) => a.get_cluster(board, player),
            CardAbstraction::OCHS(a) => a.get_cluster(board, player),
        }
    }

    fn board_from_mask(&self) -> [u8; 7] {
        let mut board = [0u8; 7];
        let mut mask = self.initial_board_mask;
        let mut i = 2usize;
        while mask.count_ones() > 0 {
            let c = mask.trailing_zeros() as u8;
            board[i] = c;
            mask ^= 1u64 << c;
            i += 1;
        }
        board
    }

    fn abs_idx_for_round(&self, round: BettingRound) -> u8 {
        let start = self.initial_board_mask.count_ones();
        match (start, round) {
            (3, BettingRound::Flop) => 0,
            (3, BettingRound::Turn) => 1,
            (3, BettingRound::River) => 2,
            (4, BettingRound::Turn) => 0,
            (4, BettingRound::River) => 1,
            (5, BettingRound::River) => 0,
            _ => panic!(
                "no abstraction for {:?} with {} starting board cards",
                round, start
            ),
        }
    }

    /// Buckets for the entry betting round (tree `round_idx` 0).
    fn bucket_count(&self, p: usize) -> usize {
        self.abs_size(0, p as u8)
    }

    fn initial_reach(&self) -> Vec<Vec<f32>> {
        let n_players = self.hand_ranges.len();
        (0..n_players)
            .map(|p| {
                let n = self.bucket_count(p);
                vec![1.0 / n as f32; n]
            })
            .collect()
    }

    fn remap_reach(
        &self,
        op: Vec<Vec<f32>>,
        board: &[u8; 7],
        from_round: u8,
        to_round: u8,
    ) -> Vec<Vec<f32>> {
        let n_players = op.len();
        let board_mask = self.initial_board_mask
            | (0..7).fold(0u64, |m, i| {
                if board[i] != 0 {
                    m | (1u64 << board[i])
                } else {
                    m
                }
            });
        let mut new_op: Vec<Vec<f32>> = (0..n_players)
            .map(|p| vec![0.0f32; self.abs_size(to_round, p as u8)])
            .collect();

        for p in 0..n_players {
            let mut bucket_hand_count = vec![0usize; op[p].len()];
            for hand in &self.hand_ranges[p].hands {
                if hand_conflicts(*hand, board_mask) {
                    continue;
                }
                let mut cards = *board;
                cards[0] = hand.0;
                cards[1] = hand.1;
                let bt = self.get_cluster(from_round, p as u8, &cards);
                if bt < bucket_hand_count.len() {
                    bucket_hand_count[bt] += 1;
                }
            }
            for hand in &self.hand_ranges[p].hands {
                if hand_conflicts(*hand, board_mask) {
                    continue;
                }
                let mut cards = *board;
                cards[0] = hand.0;
                cards[1] = hand.1;
                let bt = self.get_cluster(from_round, p as u8, &cards);
                let br = self.get_cluster(to_round, p as u8, &cards);
                if bt < op[p].len() && bucket_hand_count[bt] > 0 {
                    new_op[p][br] += op[p][bt] / bucket_hand_count[bt] as f32;
                }
            }
        }
        new_op
    }

    fn marginalize_chance(
        &self,
        child: NodeId,
        op: Vec<Vec<f32>>,
        board: [u8; 7],
        from_round: u8,
        br_walk: bool,
    ) -> Vec<Vec<f32>> {
        let to_round = from_round + 1;
        let n_players = op.len();
        let out_sizes: Vec<usize> = (0..n_players)
            .map(|p| self.abs_size(to_round, p as u8))
            .collect();
        let mut accum: Vec<Vec<f32>> = (0..n_players)
            .map(|p| vec![0.0f32; out_sizes[p]])
            .collect();
        let used = (0..7).fold(self.initial_board_mask, |m, i| {
            if board[i] != 0 {
                m | (1u64 << board[i])
            } else {
                m
            }
        });
        let mut n_cards = 0f32;
        for card in 0..CARD_COUNT as u8 {
            if used & (1u64 << card) != 0 {
                continue;
            }
            n_cards += 1.0;
            let mut next_board = board;
            let comm = (2..7).filter(|&i| board[i] != 0).count();
            let slot = 2 + comm;
            next_board[slot] = card;
            let new_op = self.remap_reach(op.clone(), &next_board, from_round, to_round);
            let child_res = if br_walk {
                self.abstract_br(child, new_op, next_board)
            } else {
                self.abstract_ev(child, new_op, next_board)
            };
            for p in 0..n_players {
                for b in 0..child_res[p].len() {
                    accum[p][b] += child_res[p][b];
                }
            }
        }
        if n_cards > 0.0 {
            for p in 0..n_players {
                for b in 0..accum[p].len() {
                    accum[p][b] /= n_cards;
                }
            }
        }
        accum
    }

    fn player_terminal_payoff(
        &self,
        tn: &TerminalNode,
        player: u8,
        scores: Option<&[u16]>,
    ) -> f32 {
        let my_wager = tn.player_wagers.get(usize::from(player)).copied().unwrap_or(0) as f32;
        match tn.ttype {
            TerminalType::UNCONTESTED => {
                if player == tn.last_to_act {
                    tn.value as f32 - my_wager
                } else {
                    -my_wager
                }
            }
            TerminalType::ALLIN | TerminalType::SHOWDOWN => {
                if let Some(scores) = scores {
                    let my_score = scores[usize::from(player)];
                    let any_higher = scores.iter().any(|&s| s > my_score);
                    if !any_higher {
                        tn.value as f32 - my_wager
                    } else {
                        -my_wager
                    }
                } else {
                    // Incomplete board (e.g. turn all-in): fall back to pot split.
                    let n = tn.player_wagers.len().max(1) as f32;
                    if player == 0 {
                        tn.value as f32 / n - my_wager
                    } else {
                        -my_wager
                    }
                }
            }
        }
    }

    fn evaluate_combo_scores(&self, hands: &[HoleCards], board: &[u8; 7]) -> Vec<u16> {
        let cards = CARDS.get().expect("CARDS not initialized");
        hands
            .iter()
            .map(|h| {
                let mut hand = Hand::empty();
                hand += cards[usize::from(h.0)];
                hand += cards[usize::from(h.1)];
                for i in 2..7 {
                    if board[i] != 0 {
                        hand += cards[usize::from(board[i])];
                    }
                }
                evaluate(&hand)
            })
            .collect()
    }

    fn terminal_payoffs_from_combos(
        &self,
        tn: &TerminalNode,
        op: &[Vec<f32>],
        board: &[u8; 7],
        round_idx: u8,
    ) -> Vec<Vec<f32>> {
        let n_players = op.len();
        let mut res: Vec<Vec<f32>> = (0..n_players)
            .map(|p| vec![0.0f32; op[p].len()])
            .collect();
        let mut denom: Vec<Vec<f32>> = (0..n_players)
            .map(|p| vec![0.0f32; op[p].len()])
            .collect();

        let board_mask = (0..7).fold(0u64, |m, i| {
            if board[i] != 0 {
                m | (1u64 << board[i])
            } else {
                m
            }
        });
        let combos = generate_all_hole_card_combos(board_mask, &self.hand_ranges);
        let n_comm = (2..7).filter(|&i| board[i] != 0).count();
        let use_eval = matches!(
            tn.ttype,
            TerminalType::SHOWDOWN | TerminalType::ALLIN
        ) && n_comm >= 5;

        for combo in combos {
            let mut weight = 1.0f32;
            let mut buckets = Vec::with_capacity(n_players);
            for p in 0..n_players {
                let mut cards = *board;
                cards[0] = combo.hands[p].0;
                cards[1] = combo.hands[p].1;
                let b = self.get_cluster(round_idx, p as u8, &cards);
                buckets.push(b);
                weight *= op[p][b];
            }
            if weight == 0.0 {
                continue;
            }
            let scores = if use_eval {
                Some(self.evaluate_combo_scores(&combo.hands, board))
            } else {
                None
            };
            for p in 0..n_players {
                let payoff = self.player_terminal_payoff(tn, p as u8, scores.as_deref());
                res[p][buckets[p]] += weight * payoff;
                denom[p][buckets[p]] += weight;
            }
        }
        for p in 0..n_players {
            for b in 0..res[p].len() {
                if denom[p][b] > 0.0 {
                    res[p][b] /= denom[p][b];
                }
            }
        }
        res
    }

    /// Compute (ev, best_response) in one pass. Two tree walks (one
    /// for EV under the average strategy, one for the per-player BR)
    /// — both O(tree size). Returned as a tuple; the convergence
    /// writer reads both into a single `Sample`.
    pub fn compute_sample_inputs(&self) -> (Vec<f32>, Vec<f32>) {
        (self.calc_ev(), self.calc_br())
    }

    fn abstract_br(&self, curr_node: NodeId, op: Vec<Vec<f32>>, board: [u8; 7]) -> Vec<Vec<f32>> {
        let node = self.game_tree.get_node(curr_node);
        match &node.data {
            GameTreeNode::Terminal(_) => self.abstract_br_terminal(curr_node, op, board),
            GameTreeNode::PublicChance(_) => {
                let child = node.children[0];
                let from_round = match &self.game_tree.get_node(child).data {
                    GameTreeNode::Action(an) => an.round_idx.saturating_sub(1),
                    _ => 0,
                };
                self.marginalize_chance(child, op, board, from_round, true)
            }
            GameTreeNode::PrivateChance => self.abstract_br(node.children[0], op, board),
            _ => self.abstract_br_infoset(curr_node, op, board),
        }
    }

    /// EV walker: identical to `abstract_br` but at the actor's decision
    /// node we take the expected value under the average strategy.
    fn abstract_ev(&self, curr_node: NodeId, op: Vec<Vec<f32>>, board: [u8; 7]) -> Vec<Vec<f32>> {
        let node = self.game_tree.get_node(curr_node);
        match &node.data {
            GameTreeNode::Terminal(_) => self.abstract_br_terminal(curr_node, op, board),
            GameTreeNode::PublicChance(_) => {
                let child = node.children[0];
                let from_round = match &self.game_tree.get_node(child).data {
                    GameTreeNode::Action(an) => an.round_idx.saturating_sub(1),
                    _ => 0,
                };
                self.marginalize_chance(child, op, board, from_round, false)
            }
            GameTreeNode::PrivateChance => self.abstract_ev(node.children[0], op, board),
            _ => self.abstract_ev_infoset(curr_node, op, board),
        }
    }

    fn abstract_br_infoset(
        &self,
        curr_node: NodeId,
        op: Vec<Vec<f32>>,
        board: [u8; 7],
    ) -> Vec<Vec<f32>> {
        let node = self.game_tree.get_node(curr_node);
        match &node.data {
            GameTreeNode::Action(an) => {
                let info_idx = an.index;
                let n_buckets = match &self.card_abs[usize::from(an.round_idx)] {
                    CardAbstraction::ISOMORPHIC(card_abs) => card_abs.get_size(an.player),
                    CardAbstraction::EMD(card_abs) => card_abs.get_size(an.player),
                    CardAbstraction::OCHS(card_abs) => card_abs.get_size(an.player),
                };

                let mut probabilites: Vec<Vec<f32>> = Vec::new();
                for i in 0..n_buckets {
                    probabilites.push(self.infosets[info_idx].final_strategy_or_uniform(i));
                }

                let player = usize::from(an.player);

                let mut payoffs: Vec<Vec<Vec<f32>>> = Vec::with_capacity(node.children.len());
                for a in 0..node.children.len() {
                    let mut newop = op.clone();
                    for b in 0..n_buckets.min(newop[player].len()) {
                        newop[player][b] *= probabilites[b][a];
                    }
                    payoffs.push(self.abstract_br(node.children[a], newop, board));
                }

                let n_players = op.len();
                let mut res: Vec<Vec<f32>> =
                    (0..n_players).map(|p| vec![0.0f32; op[p].len()]).collect();

                let mut max_a = 0usize;
                let mut max_score = f32::NEG_INFINITY;
                for a in 0..node.children.len() {
                    let score: f32 = (0..op[player].len())
                        .map(|b| payoffs[a][player][b] * op[player][b])
                        .sum();
                    if score > max_score {
                        max_score = score;
                        max_a = a;
                    }
                }
                for p in 0..n_players {
                    for b in 0..op[p].len() {
                        res[p][b] = payoffs[max_a][p][b];
                    }
                }
                return res;
            },
            _ => panic!("error")
        }
    }

    /// Like `abstract_br_infoset` but at the actor's decision we
    /// take the expected value under the average strategy (i.e. the
    /// reach-weighted sum of child payoffs).
    fn abstract_ev_infoset(
        &self,
        curr_node: NodeId,
        op: Vec<Vec<f32>>,
        board: [u8; 7],
    ) -> Vec<Vec<f32>> {
        let node = self.game_tree.get_node(curr_node);
        match &node.data {
            GameTreeNode::Action(an) => {
                let info_idx = an.index;
                let n_buckets = match &self.card_abs[usize::from(an.round_idx)] {
                    CardAbstraction::ISOMORPHIC(card_abs) => card_abs.get_size(an.player),
                    CardAbstraction::EMD(card_abs) => card_abs.get_size(an.player),
                    CardAbstraction::OCHS(card_abs) => card_abs.get_size(an.player),
                };
                let mut probabilites: Vec<Vec<f32>> = Vec::new();
                for i in 0..n_buckets {
                    probabilites.push(self.infosets[info_idx].final_strategy_or_uniform(i));
                }
                let n_players = op.len();
                let player = usize::from(an.player);

                let mut child_weight: Vec<f32> = vec![0.0; node.children.len()];
                let mut payoffs: Vec<Vec<Vec<f32>>> = Vec::with_capacity(node.children.len());
                for a in 0..node.children.len() {
                    let mut newop = op.clone();
                    for b in 0..n_buckets.min(newop[player].len()) {
                        newop[player][b] *= probabilites[b][a];
                    }
                    child_weight[a] = (0..n_buckets.min(op[player].len()))
                        .map(|b| probabilites[b][a] * op[player][b])
                        .sum();
                    payoffs.push(self.abstract_ev(node.children[a], newop, board));
                }

                let mut res: Vec<Vec<f32>> =
                    (0..n_players).map(|p| vec![0.0f32; op[p].len()]).collect();
                let total_weight: f32 = child_weight.iter().sum();
                let inv = if total_weight > 0.0 {
                    1.0 / total_weight
                } else {
                    0.0
                };
                for a in 0..node.children.len() {
                    for p in 0..n_players {
                        for b in 0..op[p].len() {
                            res[p][b] += child_weight[a] * payoffs[a][p][b] * inv;
                        }
                    }
                }
                return res;
            },
            _ => panic!("error")
        }
    }

    fn abstract_br_terminal(
        &self,
        curr_node: NodeId,
        op: Vec<Vec<f32>>,
        board: [u8; 7],
    ) -> Vec<Vec<f32>> {
        let node = self.game_tree.get_node(curr_node);
        match &node.data {
            GameTreeNode::Terminal(tn) => {
                let round_idx = self.abs_idx_for_round(tn.round);
                self.terminal_payoffs_from_combos(tn, &op, &board, round_idx)
            }
            _ => panic!("abstract_br_terminal called on non-terminal node"),
        }
    }
}

/// Filter for extracting hero samples along a specific action line.
#[derive(Debug, Clone, Default)]
pub struct HeroSampleQuery {
    /// When set, only traverse the turn (or river) chance branch dealing this card.
    pub turn_card: Option<u8>,
    /// When true, follow only check-check on the flop betting round.
    pub require_flop_checks: bool,
}

impl MCCFRTrainer {
    /// One hero decision node extracted after training (benchmark harness).
    pub fn collect_hero_samples(
        &self,
        options: &Options,
        hero: HoleCards,
        hero_player: u8,
        target_street: BettingRound,
    ) -> Vec<HeroDecisionSample> {
        self.collect_hero_samples_at_path(
            options,
            hero,
            hero_player,
            target_street,
            &HeroSampleQuery::default(),
        )
    }

    pub fn collect_hero_samples_at_path(
        &self,
        options: &Options,
        hero: HoleCards,
        hero_player: u8,
        target_street: BettingRound,
        query: &HeroSampleQuery,
    ) -> Vec<HeroDecisionSample> {
        let mut out = Vec::new();
        let mut stack: Vec<(NodeId, GameState, Vec<u8>)> =
            vec![(0, GameState::from(options), Vec::new())];

        while let Some((node, state, dealt)) = stack.pop() {
            let tree_node = self.game_tree.get_node(node);
            match &tree_node.data {
                GameTreeNode::PrivateChance => {
                    if let Some(&child) = tree_node.children.first() {
                        stack.push((child, state, dealt));
                    }
                }
                GameTreeNode::PublicChance(pc) => {
                    if let Some(&child) = tree_node.children.first() {
                        let mut next_dealt = dealt.clone();
                        if pc.round == BettingRound::Turn {
                            if let Some(tc) = query.turn_card {
                                if hero.0 == tc || hero.1 == tc {
                                    continue;
                                }
                                if self.initial_board_mask & (1u64 << tc) != 0 {
                                    continue;
                                }
                                next_dealt.push(tc);
                            }
                        }
                        stack.push((child, state.to_next_street(), next_dealt));
                    }
                }
                GameTreeNode::Action(an) => {
                    if an.player == hero_player && state.round == target_street {
                        if let Some(sample) = self.extract_hero_sample(
                            an,
                            &state,
                            hero,
                            hero_player,
                            &dealt,
                        ) {
                            out.push(sample);
                        }
                    }
                    for (i, action) in an.actions.iter().enumerate() {
                        if query.require_flop_checks
                            && state.round == BettingRound::Flop
                            && !matches!(action, Action::Check)
                        {
                            continue;
                        }
                        let next = state.apply_action(action);
                        stack.push((tree_node.children[i], next, dealt.clone()));
                    }
                }
                GameTreeNode::Terminal(_) => {}
            }
        }
        out
    }

    fn board_mask_with_dealt(&self, dealt: &[u8]) -> u64 {
        let mut mask = self.initial_board_mask;
        for &c in dealt {
            mask |= 1u64 << c;
        }
        mask
    }

    fn board_array_with_dealt(&self, dealt: &[u8]) -> [u8; 7] {
        let mut board = self.board_from_mask();
        let mut slot = 2 + self.initial_board_mask.count_ones() as usize;
        for &c in dealt {
            board[slot] = c;
            slot += 1;
        }
        board
    }

    fn extract_hero_sample(
        &self,
        an: &crate::nodes::ActionNode,
        state: &GameState,
        hero: HoleCards,
        hero_player: u8,
        dealt: &[u8],
    ) -> Option<HeroDecisionSample> {
        const CHIPS_PER_BB: f32 = 100.0;
        let round_idx = self.abs_idx_for_round(state.round);
        let mut cards = self.board_array_with_dealt(dealt);
        cards[0] = hero.0;
        cards[1] = hero.1;
        let cluster = self.get_cluster(round_idx, hero_player, &cards);
        let strategy = self.infosets[an.index].query_strategy(cluster);

        let mut fold_p = 0.0f32;
        let mut call_p = 0.0f32;
        let mut raise_p = 0.0f32;
        let mut raise_amounts: Vec<(f32, f32)> = Vec::new();
        let pot_bb = state.pot as f32 / CHIPS_PER_BB;

        for (i, action) in an.actions.iter().enumerate() {
            let p = strategy.get(i).copied().unwrap_or(0.0);
            match action {
                Action::Fold => fold_p += p,
                Action::Check | Action::Call => call_p += p,
                Action::Bet(frac) => {
                    raise_p += p;
                    raise_amounts.push((*frac as f32, p));
                }
                Action::Raise(mult) => {
                    raise_p += p;
                    let target =
                        (state.highest_wager() as f64 * mult) as u32;
                    let hero_wager = state.players[usize::from(hero_player)].wager;
                    let increment = target.saturating_sub(hero_wager);
                    let pot_pct = if pot_bb > 0.0 {
                        (increment as f32 / CHIPS_PER_BB) / pot_bb
                    } else {
                        1.0
                    };
                    raise_amounts.push((pot_pct, p));
                }
            }
        }

        let total = fold_p + call_p + raise_p;
        if total < 1e-6 {
            return None;
        }
        fold_p /= total;
        call_p /= total;
        raise_p /= total;

        let raise_multipliers = [0.50_f32, 0.75, 1.00];
        let n_raise = raise_multipliers.len() + 1;
        let mut rprobs = vec![0.01_f32; n_raise];
        for (pot_pct, prob) in &raise_amounts {
            let mut best_idx = 0usize;
            let mut best_dist = f32::MAX;
            for (i, m) in raise_multipliers.iter().enumerate() {
                let d = (pot_pct - m).abs();
                if d < best_dist {
                    best_dist = d;
                    best_idx = i;
                }
            }
            if *pot_pct > 2.5 {
                best_idx = n_raise - 1;
            }
            rprobs[best_idx] += prob;
        }
        let rp_total: f32 = rprobs.iter().sum();
        if rp_total > 1e-6 {
            for p in rprobs.iter_mut() {
                *p /= rp_total;
            }
        }

        let me = &state.players[usize::from(hero_player)];
        let call_cost_bb =
            (state.highest_wager().saturating_sub(me.wager) as f32) / CHIPS_PER_BB;

        let street = match state.round {
            BettingRound::Flop => "flop",
            BettingRound::Turn => "turn",
            BettingRound::River => "river",
        };

        Some(HeroDecisionSample {
            board: board_mask_to_strings(self.board_mask_with_dealt(dealt)),
            street: street.to_string(),
            pot_bb,
            call_cost_bb,
            action_probs: [fold_p, call_p, raise_p],
            raise_probs: [
                rprobs[0],
                rprobs[1],
                rprobs[2],
                rprobs.get(3).copied().unwrap_or(0.01),
            ],
        })
    }
}

/// Hero decision sample (compatible with rjeans `TrainingSample` scoring).
#[derive(Debug, Clone)]
pub struct HeroDecisionSample {
    pub board: Vec<String>,
    pub street: String,
    pub pot_bb: f32,
    pub call_cost_bb: f32,
    pub action_probs: [f32; 3],
    pub raise_probs: [f32; 4],
}

fn board_mask_to_strings(mask: u64) -> Vec<String> {
    let mut cards = Vec::new();
    let mut m = mask;
    while m.count_ones() > 0 {
        let c = m.trailing_zeros() as u8;
        cards.push(card_idx_to_str(c));
        m ^= 1u64 << c;
    }
    cards
}

fn card_idx_to_str(card: u8) -> String {
    let rank = usize::from(card >> 2);
    let suit = usize::from(card & 3);
    format!(
        "{}{}",
        RANK_TO_CHAR[rank],
        SUIT_TO_CHAR[suit]
    )
}

/// convergence.json writer. One `Sample` is emitted per call; the
/// `Recorder` accumulates samples and flushes them to disk in append
/// mode so a partially-written file can still be read.
///
/// Schema: see `docs/convergence_schema.md` (v1.0).
pub mod convergence {
    use serde_json::json;
    use std::fs::{File, OpenOptions};
    use std::io::{BufWriter, Write};
    use std::path::Path;

    const SCHEMA_VERSION: &str = "1.0";

    #[derive(Debug, Clone)]
    pub struct Sample {
        pub iter: u64,
        pub t_seconds: f64,
        pub depth_tier_bb: u32,
        pub n_players: usize,
        pub ev: Vec<f32>,
        pub best_response: Vec<f32>,
        pub memory_mb: u64,
        pub n_threads: usize,
        pub stop_reason: Option<String>,
    }

    impl Sample {
        pub fn exploitability_mbb_per_hand(&self) -> Vec<f32> {
            self.best_response
                .iter()
                .zip(self.ev.iter())
                .map(|(br, ev)| (br - ev) * 1000.0)
                .collect()
        }

        pub fn exploitability_max_mbb_per_hand(&self) -> f32 {
            self.exploitability_mbb_per_hand()
                .into_iter()
                .fold(f32::NEG_INFINITY, f32::max)
        }

        pub fn to_json(&self) -> serde_json::Value {
            let eps = self.exploitability_mbb_per_hand();
            json!({
                "schema_version": SCHEMA_VERSION,
                "iter": self.iter,
                "t_seconds": self.t_seconds,
                "depth_tier_bb": self.depth_tier_bb,
                "n_players": self.n_players,
                "ev": self.ev,
                "best_response": self.best_response,
                "exploitability_mbb_per_hand": eps,
                "exploitability_max_mbb_per_hand": self.exploitability_max_mbb_per_hand(),
                "memory_mb": self.memory_mb,
                "n_threads": self.n_threads,
                "stop_reason": self.stop_reason,
            })
        }
    }

    /// Append-only writer for `convergence.jsonl` (one JSON object per
    /// line). The file is created on first write if it doesn't exist.
    pub struct Recorder {
        path: std::path::PathBuf,
    }

    impl Recorder {
        pub fn new<P: AsRef<Path>>(path: P) -> Self {
            Recorder {
                path: path.as_ref().to_path_buf(),
            }
        }

        pub fn write(&self, sample: &Sample) -> std::io::Result<()> {
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.path)?;
            let mut w = BufWriter::new(file);
            let line = serde_json::to_string(&sample.to_json())
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
            writeln!(w, "{}", line)?;
            w.flush()?;
            Ok(())
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::env;

        #[test]
        fn exploitability_vector() {
            let s = Sample {
                iter: 1000,
                t_seconds: 1.0,
                depth_tier_bb: 20,
                n_players: 2,
                ev: vec![0.012, -0.012],
                best_response: vec![0.045, 0.040],
                memory_mb: 100,
                n_threads: 8,
                stop_reason: None,
            };
            let eps = s.exploitability_mbb_per_hand();
            // (0.045 - 0.012) * 1000 = 33; (0.040 - (-0.012)) * 1000 = 52
            assert!((eps[0] - 33.0).abs() < 1e-3);
            assert!((eps[1] - 52.0).abs() < 1e-3);
            assert!((s.exploitability_max_mbb_per_hand() - 52.0).abs() < 1e-3);
        }

        #[test]
        fn recorder_appends() {
            let dir = env::temp_dir().join("rustsolver-convergence-test");
            std::fs::create_dir_all(&dir).unwrap();
            let path = dir.join("test.jsonl");
            let _ = std::fs::remove_file(&path);
            let r = Recorder::new(&path);
            for i in 0..3 {
                r.write(&Sample {
                    iter: i,
                    t_seconds: i as f64,
                    depth_tier_bb: 20,
                    n_players: 2,
                    ev: vec![0.0, 0.0],
                    best_response: vec![0.001, 0.001],
                    memory_mb: 0,
                    n_threads: 8,
                    stop_reason: None,
                })
                .unwrap();
            }
            let contents = std::fs::read_to_string(&path).unwrap();
            let lines: Vec<&str> = contents.lines().collect();
            assert_eq!(lines.len(), 3);
            // Each line is a parseable JSON object.
            for line in &lines {
                let _: serde_json::Value = serde_json::from_str(line).unwrap();
            }
            std::fs::remove_file(&path).unwrap();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::default_turn_solve;
    use rust_poker::hand_evaluator::init_cards;
    use rust_poker::hand_range::get_card_mask;

    /// P0.5 smoke test: ensure `MCCFRTrainer::init` builds cleanly and
    /// `train()` runs end-to-end with finite BR and regret mutation.
    ///
    /// Requires:
    /// - `init_cards()` called once on the main thread before
    ///   `trainer.train()` (rust_poker 0.1.5 lazy_static race fix)
    /// - `OUT_DIR` set so the evaluator can find `offset_table.dat`
    ///   (the build script writes it to `target/release/deps/`)
    fn setup_out_dir() {
        if std::env::var("OUT_DIR").is_err() {
            let candidate = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("target/release/deps");
            if candidate.join("offset_table.dat").exists() {
                std::env::set_var("OUT_DIR", &candidate);
            }
        }
    }

    #[test]
    fn init_builds_clean() {
        setup_out_dir();
        init_cards();
        let options = default_turn_solve();
        let trainer = MCCFRTrainer::init(options);

        // Initial regrets should all be zero.
        let mut non_zero = 0usize;
        for row in trainer.infosets.iter() {
            for infoset in row.initialized_infosets() {
                for i in 0..infoset.n_actions() {
                    if infoset.regret(i) != 0 {
                        non_zero += 1;
                    }
                }
            }
        }
        assert_eq!(non_zero, 0, "fresh infosets should start with zero regrets");
        assert_eq!(
            trainer.infosets.allocated_count(),
            0,
            "sparse table should allocate no Infoset storage before training"
        );

        // Game tree should have at least one node.
        assert!(trainer.game_tree.len() > 0, "game tree should be non-empty");

        // Card abstraction: one entry per betting round in the tree
        // (turn+river for turn-entry scenarios).
        assert_eq!(
            trainer.card_abs.len(),
            2,
            "turn-entry should have turn + river abstractions"
        );

        // Infoset table should have at least one row.
        assert!(trainer.infosets.len() > 0, "infoset table should be non-empty");
    }

    #[test]
    fn train_runs_finite() {
        setup_out_dir();
        init_cards();
        let options = default_turn_solve();
        let mut trainer = MCCFRTrainer::init(options);

        // Snapshot non-zero regrets before training.
        let mut before = 0usize;
        for row in trainer.infosets.iter() {
            for infoset in row.initialized_infosets() {
                for i in 0..infoset.n_actions() {
                    if infoset.regret(i) != 0 {
                        before += 1;
                    }
                }
            }
        }
        assert_eq!(before, 0, "fresh infosets should start with zero regrets");

        // 1000 iters with external sampling (P9.5): one traverser per
        // iteration, rotating across players. 2000 iters worth of
        // per-player updates in 2p would need 4000 total; 1000 is
        // still enough to move regrets on the small turn tree.
        trainer.train(2_000);

        // After training, BR values should be finite.
        let br = trainer.calc_br();
        assert_eq!(br.len(), 2, "calc_br should return 2 floats (2p)");
        for (i, v) in br.iter().enumerate() {
            assert!(v.is_finite(), "br[{}] = {} is not finite", i, v);
        }

        // At least one infoset's regret should have moved off zero.
        let mut after = 0usize;
        for row in trainer.infosets.iter() {
            for infoset in row.initialized_infosets() {
                for i in 0..infoset.n_actions() {
                    if infoset.regret(i) != 0 {
                        after += 1;
                    }
                }
            }
        }
        assert!(
            after > 0,
            "no regrets moved off zero after 2000 iters (got 0)"
        );
    }

    /// P9.5: one external-sampling iteration walks the tree once.
    #[test]
    fn external_sampling_one_iter_updates_regrets() {
        setup_out_dir();
        init_cards();
        let options = default_turn_solve();
        let trainer = MCCFRTrainer::init(options);
        let mut rng = SmallRng::from_rng(thread_rng()).unwrap();
        trainer.run_one_iteration(&mut rng, 0, false, None);
        let mut moved = 0usize;
        for row in trainer.infosets.iter() {
            for infoset in row.initialized_infosets() {
                for i in 0..infoset.n_actions() {
                    if infoset.regret(i) != 0 {
                        moved += 1;
                    }
                }
            }
        }
        assert!(
            moved > 0,
            "single external-sampling iteration should update at least one regret"
        );
    }

    /// P9.1 / P10.1: known hand ranks at SHOWDOWN via the BR terminal walker.
    #[test]
    fn showdown_eval_aa_beats_22() {
        setup_out_dir();
        init_cards();
        use rust_poker::hand_range::HandRange;

        let mut options = default_turn_solve();
        // Force a single combo per player on a fixed 5-card board.
        options.board_mask = get_card_mask("4d5d7s3cKs");
        options.hand_ranges = vec![
            HandRange::from_string("AdAc".to_string()),
            HandRange::from_string("2d2c".to_string()),
        ];
        options.action_abstraction.bet_sizes = vec![vec![0.5]];
        options.action_abstraction.raise_sizes = vec![vec![3.0]];

        let trainer = MCCFRTrainer::init(options);
        let br = trainer.calc_br();
        assert!(br[0] > br[1], "AA should beat 22 in BR ordering: {:?} vs {:?}", br[0], br[1]);
    }

    #[test]
    fn showdown_eval_aks_beats_72o() {
        setup_out_dir();
        init_cards();
        use rust_poker::hand_range::HandRange;

        let mut options = default_turn_solve();
        options.board_mask = get_card_mask("4d5d7s3c2h");
        options.hand_ranges = vec![
            HandRange::from_string("AsKs".to_string()),
            HandRange::from_string("7d2c".to_string()),
        ];
        options.action_abstraction.bet_sizes = vec![vec![0.5]];
        options.action_abstraction.raise_sizes = vec![vec![3.0]];

        let trainer = MCCFRTrainer::init(options);
        let br = trainer.calc_br();
        assert!(
            br[0] > br[1],
            "AKs should beat 72o in BR ordering: {:?} vs {:?}",
            br[0],
            br[1]
        );
    }
}
