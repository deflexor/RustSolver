use std::sync::atomic::{AtomicI32, Ordering};
use crate::tree::{Tree, NodeId};
use crate::card_abstraction::{CardAbstraction, ICardAbstraction, ISOMORPHIC, EMD, OCHS};
use crate::nodes::GameTreeNode;

/// Container for infosets. Indexed by `[an.index][cluster_idx]`. The
/// `regrets` and `strategy_sum` fields are interior-mutable atomic
/// storage so multiple MCCFR worker threads can update them through a
/// shared `&InfosetTable` reference without `unsafe`.
pub type InfosetTable = Vec<Vec<Infoset>>;

pub fn create_infosets(n_actions: usize, tree: &Tree<GameTreeNode>, card_abs: &Vec<CardAbstraction>) -> InfosetTable {
    let mut infosets: Vec<Vec<Infoset>> = Vec::new();
    for _ in 0..n_actions {
        infosets.push(Vec::new());
    }
    create_infosets_rec(card_abs, tree, &mut infosets, 0);
    return infosets;
}

fn create_infosets_rec(
        card_abs: &Vec<CardAbstraction>,
        tree: &Tree<GameTreeNode>,
        infosets: &mut InfosetTable,
        node: NodeId) {
    let node = tree.get_node(node);
    match &node.data {
        GameTreeNode::Action(an) => {
            let cluster_size = match &card_abs[usize::from(an.round_idx)] {
                CardAbstraction::EMD(card_abs) => card_abs.get_size(an.player),
                CardAbstraction::OCHS(card_abs) => card_abs.get_size(an.player),
                CardAbstraction::ISOMORPHIC(card_abs) => card_abs.get_size(an.player),
            };
            let n_actions = node.children.len();
            for _ in 0..cluster_size {
                infosets[an.index].push(Infoset::init(n_actions));
            }
            for i in 0..n_actions {
                create_infosets_rec(card_abs, tree, infosets, node.children[i]);
            }
        },
        GameTreeNode::PrivateChance => {
            create_infosets_rec(card_abs, tree, infosets, node.children[0]);
        },
        GameTreeNode::PublicChance(_) => {
            create_infosets_rec(card_abs, tree, infosets, node.children[0]);
        },
        GameTreeNode::Terminal(_) => {},
    }
}

#[derive(Debug)]
pub struct Infoset {
    /// Cumulative regret per action. Atomic so MCCFR workers can update
    /// through `&Infoset` without external locking.
    pub regrets: Box<[AtomicI32]>,
    /// Cumulative strategy weight per action (used to compute the
    /// average strategy at the end of training).
    pub strategy_sum: Box<[AtomicI32]>,
}

impl Infoset {
    pub fn init(n_actions: usize) -> Infoset {
        Infoset {
            regrets: (0..n_actions).map(|_| AtomicI32::new(0)).collect(),
            strategy_sum: (0..n_actions).map(|_| AtomicI32::new(0)).collect(),
        }
    }

    pub fn n_actions(&self) -> usize {
        self.regrets.len()
    }

    /// Read a regret value (relaxed ordering; safe for visualization
    /// and BR computation; not for cross-thread synchronization).
    pub fn regret(&self, i: usize) -> i32 {
        self.regrets[i].load(Ordering::Relaxed)
    }

    /// Read a strategy_sum value.
    pub fn strategy_sum_at(&self, i: usize) -> i32 {
        self.strategy_sum[i].load(Ordering::Relaxed)
    }

    /// Snapshot of all regrets, used by `get_strategy` and tests.
    fn regrets_snapshot(&self) -> Vec<i32> {
        self.regrets.iter().map(|r| r.load(Ordering::Relaxed)).collect()
    }

    fn strategy_sum_snapshot(&self) -> Vec<i32> {
        self.strategy_sum.iter().map(|r| r.load(Ordering::Relaxed)).collect()
    }

    /// Current strategy via regret matching. Reads the atomic regrets
    /// in a non-atomic loop; this is OK because the resulting strategy
    /// is approximate and the trainer recomputes it every iteration.
    pub fn get_strategy(&self) -> Vec<f32> {
        let regrets = self.regrets_snapshot();
        let n_actions = regrets.len();
        let mut strategy = vec![0.0; n_actions];
        let mut norm_sum = 0.0f32;
        for i in 0..n_actions {
            if regrets[i] > 0 {
                norm_sum += regrets[i] as f32;
            }
        }
        for i in 0..n_actions {
            if norm_sum > 0.0 {
                if regrets[i] > 0 {
                    strategy[i] = regrets[i] as f32 / norm_sum;
                }
            } else {
                strategy[i] = 1.0 / (n_actions as f32);
            }
        }
        return strategy;
    }

    /// Average strategy from the cumulative strategy_sum.
    pub fn get_final_strategy(&self) -> Vec<f32> {
        let ssum = self.strategy_sum_snapshot();
        let n_actions = ssum.len();
        let mut strategy = vec![0.0; n_actions];
        let mut norm_sum = 0.0f32;
        for i in 0..n_actions {
            if ssum[i] > 0 {
                norm_sum += ssum[i] as f32;
            }
        }
        for i in 0..n_actions {
            if norm_sum > 0.0 {
                if ssum[i] > 0 {
                    strategy[i] = ssum[i] as f32 / norm_sum;
                }
            } else {
                strategy[i] = 1.0 / (n_actions as f32);
            }
        }
        return strategy;
    }

    /// Add `delta` to `regrets[i]`. Saturates at i32::MAX/MIN to avoid
    /// overflow (CFR regret can grow unboundedly in long runs).
    pub fn add_regret(&self, i: usize, delta: i32) {
        let _ = self.regrets[i].fetch_update(Ordering::Relaxed, Ordering::Relaxed, |cur| {
            let next = (i64::from(cur) + i64::from(delta)).clamp(i32::MIN as i64, i32::MAX as i64);
            Some(next as i32)
        });
    }

    /// Add `delta` to `strategy_sum[i]`. Saturates.
    pub fn add_strategy_sum(&self, i: usize, delta: i32) {
        let _ = self.strategy_sum[i].fetch_update(Ordering::Relaxed, Ordering::Relaxed, |cur| {
            let next = (i64::from(cur) + i64::from(delta)).clamp(i32::MIN as i64, i32::MAX as i64);
            Some(next as i32)
        });
    }

    /// CFR+ floor: cap regrets[i] at 0. Used when the trainer is in
    /// CFR+ mode (P4.4).
    pub fn floor_regrets_at_zero(&self) {
        for r in self.regrets.iter() {
            r.fetch_max(0, Ordering::Relaxed);
        }
    }

    /// Linear-CFR-style discount: multiply regrets and strategy_sum
    /// by `d` (0 < d < 1). `d = p/(p+1)` is the canonical formula.
    /// All atomics, no `unsafe`.
    pub fn discount(&self, d: f32) {
        for r in self.regrets.iter() {
            r.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |cur| {
                Some((cur as f32 * d) as i32)
            });
        }
        for s in self.strategy_sum.iter() {
            s.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |cur| {
                Some((cur as f32 * d) as i32)
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_regret_saturates_at_i32_max() {
        let info = Infoset::init(1);
        info.add_regret(0, i32::MAX);
        info.add_regret(0, i32::MAX);
        assert_eq!(info.regret(0), i32::MAX);
    }

    #[test]
    fn discount_zeroes_to_zero() {
        let info = Infoset::init(2);
        info.add_regret(0, 1000);
        info.add_strategy_sum(0, 1000);
        info.discount(0.0);
        assert_eq!(info.regret(0), 0);
        assert_eq!(info.strategy_sum_at(0), 0);
    }

    #[test]
    fn floor_regrets_at_zero_negatives_to_zero() {
        let info = Infoset::init(2);
        info.add_regret(0, -100);
        info.add_regret(1, 50);
        info.floor_regrets_at_zero();
        assert_eq!(info.regret(0), 0);
        assert_eq!(info.regret(1), 50);
    }
}
