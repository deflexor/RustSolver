use std::iter::FromIterator;
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

use crate::tree::{Tree, NodeId};
use crate::nodes::{TerminalType};
use crate::tree_builder::build_game_tree;
use crate::infoset::{Infoset, InfosetTable, create_infosets};
use crate::nodes::GameTreeNode;
use crate::options::Options;
use crate::card_abstraction::{CardAbstraction, ISOMORPHIC, EMD, ICardAbstraction};
use crate::state::BettingRound;

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

fn generate_hand<R: Rng>(rng: &mut R, mut board_mask: u64, hand_ranges: &[HandRange]) -> TrainHand {
    let mut used_cards_mask = board_mask;
    let mut board = [0u8; 7];
    let mut i = 2;
    let card_dist = Uniform::from(0..52);

    while board_mask.count_ones() > 0 {
        board[i] = board_mask.trailing_zeros() as u8;
        board_mask ^= 1u64 << board_mask.trailing_zeros();
        i += 1;
    }
    while i < 7 {
        let c = card_dist.sample(rng);
        if ((1u64 << c) & used_cards_mask) == 0 {
            board[i] = c;
            used_cards_mask |= 1u64 << c;
            i += 1;
        }
    }

    let mut hands: Vec<HoleCards> = Vec::with_capacity(hand_ranges.len());
    for range in hand_ranges {
        loop {
            let c = range.hands.choose(rng).unwrap();
            let combo_mask = (1u64 << c.0) | (1u64 << c.1);
            if (combo_mask & used_cards_mask) == 0 {
                used_cards_mask |= combo_mask;
                hands.push(*c);
                break;
            }
        }
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
}

impl Default for TrainConfig {
    fn default() -> Self {
        TrainConfig {
            max_iter: 10_000_000,
            target_exploitability_mbb: None,
            convergence_interval: 100_000,
            convergence_path: None,
            cfr_plus: false,
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
}

/// Best-effort RSS estimator. On Linux reads `/proc/self/statm`; on
/// other platforms returns 0. The convergence writer uses this for
/// the `memory_mb` field of the Sample.
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

impl MCCFRTrainer {
    pub fn init(options: Options) -> Self {

        let mut hand_ranges = options.hand_ranges.to_owned();

        remove_invalid_combos(&mut hand_ranges, options.board_mask);

        let (n_actions, game_tree) = build_game_tree(&options);

        // Build the card abstraction per round. EMD requires the
        // `round_X_emd.dat` files generated by `gen_abstraction`
        // (P2.1). If a file is missing, we fall back to ISOMORPHIC
        // (every hand is its own cluster) so the solver still
        // builds. EMD is only wired in when the option's board mask
        // matches the expected board count for the round (3 cards
        // for flop, 4 for turn, 5 for river), since the EMD indexer
        // uses hardcoded 2-element / 3-or-4-or-5-element
        // `cards_per_round` arrays.
        let board_count = options.board_mask.count_ones();
        let emd_flop_path = "round_1_emd.dat";
        let emd_turn_path = "round_2_emd.dat";

        let mut card_abs: Vec<CardAbstraction> = Vec::new();
        // Round 0 = flop abstraction (used when the current street
        // is the flop, i.e. 3 cards on the board).
        if board_count == 3 && std::path::Path::new(emd_flop_path).exists() {
            card_abs.push(CardAbstraction::EMD(
                EMD::init(&hand_ranges, options.board_mask, BettingRound::Flop)
            ));
        } else {
            card_abs.push(CardAbstraction::ISOMORPHIC(
                ISOMORPHIC::init(&hand_ranges, options.board_mask, BettingRound::Flop)
            ));
        }
        // Round 1 = turn abstraction (4 cards on the board).
        if board_count == 4 && std::path::Path::new(emd_turn_path).exists() {
            card_abs.push(CardAbstraction::EMD(
                EMD::init(&hand_ranges, options.board_mask, BettingRound::Turn)
            ));
        } else {
            card_abs.push(CardAbstraction::ISOMORPHIC(
                ISOMORPHIC::init(&hand_ranges, options.board_mask, BettingRound::Turn)
            ));
        }
        // Round 2 = river abstraction (5 cards). The original
        // ISOMORPHIC::River is the only one originally wired up;
        // it remains the default. Phase 2 doesn't add a river EMD
        // because river is the street that benefits least from
        // clustering (each hand has a single unique 7-card eval).
        card_abs.push(CardAbstraction::ISOMORPHIC(
            ISOMORPHIC::init(&hand_ranges, options.board_mask, BettingRound::River)
        ));

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

    /// Run MCCFR training with the given config. `cfg` controls
    /// convergence sampling, target exploitability, and the
    /// `convergence.jsonl` output path.
    pub fn train_with_config(&mut self, cfg: &TrainConfig) {
        const PRUNE_THRESHOLD: usize = 10_000_000;
        const DISCOUNT_INTERVAL: usize = 100_000;
        const DISCOUNT_CAP: usize = 20_000_000;
        const N_THREADS: usize = 8;
        let max_iter = cfg.max_iter;
        let target_mbb = cfg.target_exploitability_mbb;
        let conv_interval = cfg.convergence_interval.max(1);

        let thread_rng = thread_rng();
        let n_players = self.hand_ranges.len();

        let t = Arc::new(AtomicCell::new(0));
        let a_self = Arc::new(self);
        let started = std::time::Instant::now();
        let recorder = cfg
            .convergence_path
            .as_ref()
            .map(|p| convergence::Recorder::new(p));

        let cfr_plus = cfg.cfr_plus;
        crossbeam::scope(|scope| {
            // Worker threads.
            for _ in 0..N_THREADS {
                let a_self = Arc::clone(&a_self);
                let mut rng = SmallRng::from_rng(thread_rng).unwrap();
                let t = t.clone();
                scope.spawn(move |_| {
                    while t.load() < max_iter {
                        let hand = generate_hand(
                                &mut rng,
                                a_self.initial_board_mask,
                                a_self.hand_ranges.as_slice());
                        let q: f32 = rng.gen();
                        for player in 0..n_players {
                            if t.load() > PRUNE_THRESHOLD && q > 0.05 {
                                a_self.mccfr(&mut rng, 0, player as u8, hand.clone(), 1f32, true, cfr_plus);
                            } else {
                                a_self.mccfr(&mut rng, 0, player as u8, hand.clone(), 1f32, false, cfr_plus);
                            }
                        }
                        t.fetch_add(1);
                    }
                });
            }

            // Discount thread (every DISCOUNT_INTERVAL iters).
            let a_self_discount = Arc::clone(&a_self);
            let t_discount = Arc::clone(&t);
            scope.spawn(move |_| {
                let mut threshold = DISCOUNT_INTERVAL;
                while t_discount.load() < max_iter {
                    let onems = time::Duration::from_millis(1);
                    thread::sleep(onems);
                    let tc = t_discount.load();
                    if tc > DISCOUNT_CAP {
                        break;
                    }
                    if tc > threshold {
                        let p = (tc / DISCOUNT_INTERVAL) as f32;
                        let d = p / (p + 1.0);
                        for row in a_self_discount.infosets.iter() {
                            for infoset in row.iter() {
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
                let depth_tier_bb = a_self_conv.depth_tier_bb;
                scope.spawn(move |_| {
                    let mut next_sample_at: u64 = conv_interval as u64;
                    let mut stop_reason: Option<String> = None;
                    while t_conv.load() < max_iter {
                        let onems = time::Duration::from_millis(1);
                        thread::sleep(onems);
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
                            memory_mb: estimate_rss_mb(),
                            n_threads: N_THREADS,
                            stop_reason: None,
                        };
                        // Check stop condition before recording.
                        if let Some(target) = target_mbb {
                            if sample.exploitability_max_mbb_per_hand() <= target {
                                stop_reason = Some("target_reached".to_string());
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
                    // Final sample with stop_reason if we exited cleanly.
                    if stop_reason.is_some() {
                        let (ev, br) = a_self_conv.compute_sample_inputs();
                        let final_sample = convergence::Sample {
                            iter: t_conv.load() as u64,
                            t_seconds: started.elapsed().as_secs_f64(),
                            depth_tier_bb,
                            n_players,
                            ev,
                            best_response: br,
                            memory_mb: estimate_rss_mb(),
                            n_threads: N_THREADS,
                            stop_reason,
                        };
                        let _ = recorder.write(&final_sample);
                    }
                });
            }
        })
        .unwrap();
    }

    fn mccfr<R: Rng>(&self,
            rng: &mut R, node_id: NodeId,
            player: u8, mut hand: TrainHand,
            cfr_reach: f32, prune: bool, cfr_plus: bool) -> f32 {

        let node = self.game_tree.get_node(node_id);
        match &node.data {
            GameTreeNode::PublicChance(_) => {
                // progress to next node
                return self.mccfr(rng, node.children[0], player, hand, cfr_reach, prune, cfr_plus);
            },
            GameTreeNode::PrivateChance => {
                // progress to next node
                return self.mccfr(rng, node.children[0], player, hand, cfr_reach, prune, cfr_plus);
            },
            GameTreeNode::Terminal(tn) => {
                match tn.ttype {
                    TerminalType::UNCONTESTED => {
                        // In 2p, last_to_act is the folding player. The
                        // other player wins the pot.
                        // In 3p, last_to_act is the last player to take
                        // a non-fold action; the others all folded.
                        // Whoever didn't fold wins; everyone else loses
                        // their wager.
                        if hand.num_players() == 2 {
                            if player == tn.last_to_act {
                                return -1.0 * (tn.value as f32);
                            } else {
                                return 1.0 * (tn.value as f32);
                            }
                        } else {
                            // 3p: each non-folding player wins their
                            // share; folding player loses. We don't
                            // track who folded in tn, so the convention
                            // here is: the actor who made the call/bet
                            // (last_to_act) is the *remaining* player if
                            // `tn.value` is positive. Since UNCONTESTED in
                            // this solver is 2p-only for now, fall back
                            // to the 2p logic with a warning that the
                            // 3p case is approximate.
                            if player == tn.last_to_act {
                                return 1.0 * (tn.value as f32);
                            } else {
                                return -1.0 * (tn.value as f32);
                            }
                        }
                    },
                    TerminalType::SHOWDOWN | TerminalType::ALLIN => {
                        // N-player showdown: compute per-player score,
                        // rank them, and pay the winner the pot.
                        let n = hand.num_players();
                        if n < 2 {
                            return 0.0;
                        }
                        let scores: Vec<u16> =
                            (0..n).map(|p| evaluate(&hand.get_hand(p as u8))).collect();
                        let my_score = scores[usize::from(player)];
                        let any_higher = scores.iter().any(|&s| s > my_score);
                        if !any_higher {
                            // I won (or tied for the win). For N-player
                            // ties, the convention here is the player
                            // gets the full pot (approximate; proper
                            // 3p ties should split the pot).
                            return 1.0 * (tn.value as f32);
                        } else {
                            return -1.0 * (tn.value as f32);
                        }
                    }
                }
            },
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
                if an.player == player {
                    let mut util = 0f32;
                    let mut utils = vec![0f32; n_actions];
                    let mut explored = vec![false; n_actions];

                    let infoset = &self.infosets[an.index][cluster_idx];
                    let strategy = infoset.get_strategy();

                    for i in 0..n_actions {
                        if prune {
                            if infoset.regret(i) > PRUNE_THRESHOLD {
                            utils[i] = self.mccfr(
                                rng, node.children[i],
                                player, hand.clone(), cfr_reach, prune, cfr_plus);
                            util += utils[i] * strategy[i];
                            explored[i] = true;
                            }
                        } else {
                        utils[i] = self.mccfr(
                            rng, node.children[i],
                            player, hand.clone(), cfr_reach, prune, cfr_plus);
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
                } else {
                    // sample one action based on distribution
                    let infoset = &self.infosets[an.index][cluster_idx];
                    let strategy = infoset.get_strategy();
                    let dist = WeightedIndex::new(&strategy).unwrap();
                    let a_idx = dist.sample(rng);
                    return self.mccfr(
                        rng, node.children[a_idx],
                        player, hand, cfr_reach * strategy[a_idx], prune, cfr_plus);
                }
            }
        }
    }

    // Non-sampling CFR (cfr) was removed in P0.4. It was dead code (the
    // external call site at line 217 was commented out) and contained an
    // unsafe raw-pointer cast into Infoset. A clean non-sampling CFR
    // implementation will be reintroduced as a separate `FullCFR` module
    // for the safe-search work in Phase 6.

    fn calc_br(&self) -> Vec<f32> {
        let n_players = self.hand_ranges.len();
        let op = vec![vec![1.0; 1]; n_players];
        let res = self.abstract_br(0, op);
        let mut out = vec![0f32; res.len()];
        for i in 0..res.len() {
            out[i] = res[i][0];
        }
        return out;
    }

    /// Average EV per player when all players use their average
    /// strategy (the same input the BR walker uses, but the actor at
    /// each infoset follows the average strategy instead of picking
    /// the best response). Returns a length-`n_players` vector.
    fn calc_ev(&self) -> Vec<f32> {
        let n_players = self.hand_ranges.len();
        let op = vec![vec![1.0; 1]; n_players];
        let res = self.abstract_ev(0, op);
        let mut out = vec![0f32; res.len()];
        for i in 0..res.len() {
            out[i] = res[i][0];
        }
        return out;
    }

    /// Compute (ev, best_response) in one pass. Two tree walks (one
    /// for EV under the average strategy, one for the per-player BR)
    /// — both O(tree size). Returned as a tuple; the convergence
    /// writer reads both into a single `Sample`.
    pub fn compute_sample_inputs(&self) -> (Vec<f32>, Vec<f32>) {
        (self.calc_ev(), self.calc_br())
    }

    fn abstract_br(&self, curr_node: NodeId, op: Vec<Vec<f32>>) -> Vec<Vec<f32>> {
        let node = self.game_tree.get_node(curr_node);
        match &node.data {
            GameTreeNode::Terminal(_) => {
                return self.abstract_br_terminal(curr_node, op);
            },
            GameTreeNode::PublicChance(_) => {
                return self.abstract_br(node.children[0], op);
            },
            GameTreeNode::PrivateChance => {
                return self.abstract_br(node.children[0], op);
            },
            _ => {
                return self.abstract_br_infoset(curr_node, op);
            }
        }
    }

    /// EV walker: identical to `abstract_br` but at the actor's
    /// decision node we take the expected value under the average
    /// strategy (sum of `probabilites[a] * payoffs[a]`) instead of
    /// `max(payoffs)`.
    fn abstract_ev(&self, curr_node: NodeId, op: Vec<Vec<f32>>) -> Vec<Vec<f32>> {
        let node = self.game_tree.get_node(curr_node);
        match &node.data {
            GameTreeNode::Terminal(_) => {
                return self.abstract_br_terminal(curr_node, op);
            },
            GameTreeNode::PublicChance(_) => {
                return self.abstract_ev(node.children[0], op);
            },
            GameTreeNode::PrivateChance => {
                return self.abstract_ev(node.children[0], op);
            },
            _ => {
                return self.abstract_ev_infoset(curr_node, op);
            }
        }
    }

    fn abstract_br_infoset(&self, curr_node: NodeId, op: Vec<Vec<f32>>) -> Vec<Vec<f32>> {
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
                    probabilites.push(self.infosets[info_idx][i].get_final_strategy());
                }

                // op[player] is currently length 1 (a single scalar
                // reach per player). The bucket dimension is tracked
                // implicitly via `probabilites` lookup; we use the
                // first bucket's strategy as the average. This is a
                // known approximation of the per-bucket reach that
                // Phase 5 will replace with sparse reach tracking.
                let player = usize::from(an.player);

                let mut payoffs: Vec<Vec<Vec<f32>>> = Vec::with_capacity(node.children.len());
                for a in 0..node.children.len() {
                    let mut newop = op.clone();
                    // The actor's reach after this action is the
                    // current reach times the average strategy. With
                    // op[player].len() == 1, we multiply the single
                    // slot by the first bucket's strategy.
                    newop[player][0] *= probabilites[0][a];
                    payoffs.push(self.abstract_br(node.children[a], newop));
                }

                let mut max_val = payoffs[0][player][0];
                let mut max_index = 0usize;
                for a in 1..node.children.len() {
                    if max_val < payoffs[a][player][0] {
                        max_val = payoffs[a][player][0];
                        max_index = a;
                    }
                }

                let n_players = op.len();
                let mut res: Vec<Vec<f32>> = vec![vec![0.0; 1]; n_players];
                res[player][0] = max_val;
                for p in 0..n_players {
                    if p != player {
                        res[p][0] = payoffs[max_index][p][0];
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
    fn abstract_ev_infoset(&self, curr_node: NodeId, op: Vec<Vec<f32>>) -> Vec<Vec<f32>> {
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
                    probabilites.push(self.infosets[info_idx][i].get_final_strategy());
                }
                let n_players = op.len();
                let player = usize::from(an.player);

                // Single-reach approximation (see abstract_br_infoset).
                // Per-action weight: prob[0][a] (the average strategy's
                // action probability under the first bucket).
                let mut child_weight: Vec<f32> = vec![0.0; node.children.len()];
                let mut payoffs: Vec<Vec<Vec<f32>>> = Vec::with_capacity(node.children.len());
                for a in 0..node.children.len() {
                    let mut newop = op.clone();
                    newop[player][0] *= probabilites[0][a];
                    child_weight[a] = probabilites[0][a];
                    payoffs.push(self.abstract_ev(node.children[a], newop));
                }

                // Per-player expected value: sum_a( weight[a] * payoffs[a][p] ).
                let mut res: Vec<Vec<f32>> = vec![vec![0.0; 1]; n_players];
                let total_weight: f32 = child_weight.iter().sum();
                let inv = if total_weight > 0.0 { 1.0 / total_weight } else { 0.0 };
                for a in 0..node.children.len() {
                    for p in 0..n_players {
                        res[p][0] += child_weight[a] * payoffs[a][p][0] * inv;
                    }
                }
                return res;
            },
            _ => panic!("error")
        }
    }

    fn abstract_br_terminal(&self, curr_node: NodeId, op: Vec<Vec<f32>>) -> Vec<Vec<f32>> {
        let node = self.game_tree.get_node(curr_node);
        match &node.data {
            GameTreeNode::Terminal(tn) => {
                let n_players = op.len();
                let mut payoffs: Vec<Vec<f32>> = vec![vec![0.0; op[0].len()]; n_players];
                let mut res: Vec<Vec<f32>> = vec![vec![0.0; 1]; n_players];
                let money_f = tn.value as f32;

                match tn.ttype {
                    TerminalType::UNCONTESTED => {
                        // Convention: last_to_act is the actor who just
                        // closed the betting (the one who didn't fold
                        // in 2p). For 3p the same convention holds as
                        // a placeholder; full 3p support lands in
                        // Phase 6.
                        let fold_player = tn.last_to_act;
                        let mut opp_ges_p = vec![0.0; n_players];
                        for p in 0..n_players {
                            // Sum reach over the OTHER players' buckets.
                            let mut opp_reach = 0.0f32;
                            for q in 0..n_players {
                                if q != p {
                                    opp_reach += op[q].iter().sum::<f32>();
                                }
                            }
                            let sign = if p == usize::from(fold_player) { -1.0 } else { 1.0 };
                            for g in 0..op[0].len() {
                                payoffs[p][g] = opp_reach * sign * money_f / (n_players as f32 - 1.0).max(1.0);
                                res[p][0] += payoffs[p][g];
                            }
                            opp_ges_p[p] = opp_reach;
                            if opp_ges_p[p] > 0.0 {
                                res[p][0] *= 1.0 / opp_ges_p[p];
                            }
                        }
                        return res;
                    },
                    _ => {
                        let mut opp_ges_p = vec![0.0; n_players];
                        for p in 0..n_players {
                            let mut opp_reach = 0.0f32;
                            for q in 0..n_players {
                                if q != p {
                                    opp_reach += op[q].iter().sum::<f32>();
                                }
                            }
                            for g in 0..op[0].len() {
                                payoffs[p][g] = opp_reach * money_f / (n_players as f32 - 1.0).max(1.0);
                                res[p][0] += payoffs[p][g];
                            }
                            opp_ges_p[p] = opp_reach;
                            if opp_ges_p[p] > 0.0 {
                                res[p][0] *= 1.0 / opp_ges_p[p];
                            }
                        }
                        return res;
                    }
                }
            },
                _ => panic!("error")
        }
    }
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
    use crate::options::{default_flop, default_turn};
    use rust_poker::hand_evaluator::init_cards;

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
        let options = default_flop();
        let trainer = MCCFRTrainer::init(options);

        // Initial regrets should all be zero.
        let mut non_zero = 0usize;
        for infoset_row in trainer.infosets.iter() {
            for infoset in infoset_row.iter() {
                for i in 0..infoset.n_actions() {
                    if infoset.regret(i) != 0 {
                        non_zero += 1;
                    }
                }
            }
        }
        assert_eq!(non_zero, 0, "fresh infosets should start with zero regrets");

        // Game tree should have at least one node.
        assert!(trainer.game_tree.len() > 0, "game tree should be non-empty");

        // Card abstraction has one entry per postflop street
        // (flop, turn, river). Round counts are stored in
        // `card_abs[round_idx]`.
        assert_eq!(trainer.card_abs.len(), 3, "should have one card abstraction per postflop street");

        // Infoset table should have at least one row.
        assert!(trainer.infosets.len() > 0, "infoset table should be non-empty");
    }

    #[test]
    fn train_runs_finite() {
        setup_out_dir();
        init_cards();
        let options = default_flop();
        let mut trainer = MCCFRTrainer::init(options);

        // Snapshot non-zero regrets before training.
        let mut before = 0usize;
        for infoset_row in trainer.infosets.iter() {
            for infoset in infoset_row.iter() {
                for i in 0..infoset.n_actions() {
                    if infoset.regret(i) != 0 {
                        before += 1;
                    }
                }
            }
        }
        assert_eq!(before, 0, "fresh infosets should start with zero regrets");

        // 1000 iters is enough to drive non-zero regrets without
        // blowing test time. The default config uses 8 worker threads.
        trainer.train(1_000);

        // After training, BR values should be finite.
        let br = trainer.calc_br();
        assert_eq!(br.len(), 2, "calc_br should return 2 floats (2p)");
        for (i, v) in br.iter().enumerate() {
            assert!(v.is_finite(), "br[{}] = {} is not finite", i, v);
        }

        // At least one infoset's regret should have moved off zero.
        let mut after = 0usize;
        for infoset_row in trainer.infosets.iter() {
            for infoset in infoset_row.iter() {
                for i in 0..infoset.n_actions() {
                    if infoset.regret(i) != 0 {
                        after += 1;
                    }
                }
            }
        }
        assert!(
            after > 0,
            "no regrets moved off zero after 1000 iters (got 0)"
        );
    }
}
