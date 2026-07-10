use std::sync::OnceLock;
use std::sync::atomic::{AtomicI32, Ordering};
use crate::tree::{Tree, NodeId};
use crate::card_abstraction::{CardAbstraction, ICardAbstraction, ISOMORPHIC, EMD, OCHS};
use crate::nodes::GameTreeNode;

/// One action-node row: `slots[b]` is bucket `b` for that infoset index.
/// Slots are reserved up front; `Infoset` atomics are allocated on first
/// write (P5.2 sparse allocation).
#[derive(Debug)]
pub struct InfosetRow {
    n_actions: usize,
    slots: Vec<InfosetSlot>,
}

impl InfosetRow {
    fn new() -> Self {
        InfosetRow {
            n_actions: 0,
            slots: Vec::new(),
        }
    }

    fn ensure_capacity(&mut self, cluster_size: usize, n_actions: usize) {
        self.n_actions = n_actions;
        if self.slots.len() < cluster_size {
            self.slots.resize_with(cluster_size, InfosetSlot::default);
        }
    }

    pub fn len(&self) -> usize {
        self.slots.len()
    }

    pub fn n_actions(&self) -> usize {
        self.n_actions
    }

    /// Allocate on first regret/strategy write.
    pub fn get_or_init(&self, cluster: usize) -> &Infoset {
        self.slots[cluster]
            .0
            .get_or_init(|| Infoset::init(self.n_actions))
    }

    pub fn get(&self, cluster: usize) -> Option<&Infoset> {
        self.slots[cluster].0.get()
    }

    /// Average strategy for BR/EV walkers; uniform if never visited.
    pub fn final_strategy_or_uniform(&self, cluster: usize) -> Vec<f32> {
        match self.get(cluster) {
            Some(infoset) => infoset.get_final_strategy(),
            None => uniform_strategy(self.n_actions),
        }
    }

    /// Current strategy for external-sampling opponent nodes.
    pub fn strategy_or_uniform(&self, cluster: usize) -> Vec<f32> {
        match self.get(cluster) {
            Some(infoset) => infoset.get_strategy(),
            None => uniform_strategy(self.n_actions),
        }
    }

    pub fn initialized_infosets(&self) -> impl Iterator<Item = &Infoset> {
        self.slots.iter().filter_map(|s| s.0.get())
    }

    pub fn allocated_count(&self) -> usize {
        self.slots.iter().filter(|s| s.0.get().is_some()).count()
    }

    pub fn total_bytes(&self) -> usize {
        let mut bytes = self.slots.capacity() * std::mem::size_of::<InfosetSlot>();
        for slot in &self.slots {
            if let Some(infoset) = slot.0.get() {
                bytes += infoset.allocated_bytes();
            }
        }
        bytes
    }
}

#[derive(Debug, Default)]
struct InfosetSlot(OnceLock<Infoset>);

/// Container for infosets. Indexed by `[action_index][cluster_idx]`.
#[derive(Debug)]
pub struct InfosetTable {
    rows: Vec<InfosetRow>,
}

impl InfosetTable {
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = &InfosetRow> {
        self.rows.iter()
    }

    pub fn total_bytes(&self) -> usize {
        self.rows.iter().map(InfosetRow::total_bytes).sum()
    }

    pub fn allocated_count(&self) -> usize {
        self.rows.iter().map(InfosetRow::allocated_count).sum()
    }
}

impl std::ops::Index<usize> for InfosetTable {
    type Output = InfosetRow;

    fn index(&self, index: usize) -> &InfosetRow {
        &self.rows[index]
    }
}

pub fn create_infosets(
    n_actions: usize,
    tree: &Tree<GameTreeNode>,
    card_abs: &Vec<CardAbstraction>,
) -> InfosetTable {
    let rows: Vec<InfosetRow> = (0..n_actions).map(|_| InfosetRow::new()).collect();
    let mut infosets = InfosetTable { rows };
    create_infosets_rec(card_abs, tree, &mut infosets, 0);
    infosets
}

fn create_infosets_rec(
    card_abs: &Vec<CardAbstraction>,
    tree: &Tree<GameTreeNode>,
    infosets: &mut InfosetTable,
    node: NodeId,
) {
    let node = tree.get_node(node);
    match &node.data {
        GameTreeNode::Action(an) => {
            let cluster_size = match &card_abs[usize::from(an.round_idx)] {
                CardAbstraction::EMD(card_abs) => card_abs.get_size(an.player),
                CardAbstraction::OCHS(card_abs) => card_abs.get_size(an.player),
                CardAbstraction::ISOMORPHIC(card_abs) => card_abs.get_size(an.player),
            };
            let n_actions = node.children.len();
            infosets.rows[an.index].ensure_capacity(cluster_size, n_actions);
            for i in 0..n_actions {
                create_infosets_rec(card_abs, tree, infosets, node.children[i]);
            }
        }
        GameTreeNode::PrivateChance => {
            create_infosets_rec(card_abs, tree, infosets, node.children[0]);
        }
        GameTreeNode::PublicChance(_) => {
            create_infosets_rec(card_abs, tree, infosets, node.children[0]);
        }
        GameTreeNode::Terminal(_) => {}
    }
}

fn uniform_strategy(n_actions: usize) -> Vec<f32> {
    vec![1.0 / n_actions as f32; n_actions]
}

#[derive(Debug)]
pub struct Infoset {
    pub regrets: Box<[AtomicI32]>,
    pub strategy_sum: Box<[AtomicI32]>,
}

impl Infoset {
    pub fn init(n_actions: usize) -> Infoset {
        Infoset {
            regrets: (0..n_actions).map(|_| AtomicI32::new(0)).collect(),
            strategy_sum: (0..n_actions).map(|_| AtomicI32::new(0)).collect(),
        }
    }

    pub fn allocated_bytes(&self) -> usize {
        self.regrets.len() * std::mem::size_of::<AtomicI32>()
            + self.strategy_sum.len() * std::mem::size_of::<AtomicI32>()
            + std::mem::size_of::<Self>()
    }

    pub fn n_actions(&self) -> usize {
        self.regrets.len()
    }

    pub fn regret(&self, i: usize) -> i32 {
        self.regrets[i].load(Ordering::Relaxed)
    }

    pub fn strategy_sum_at(&self, i: usize) -> i32 {
        self.strategy_sum[i].load(Ordering::Relaxed)
    }

    fn regrets_snapshot(&self) -> Vec<i32> {
        self.regrets.iter().map(|r| r.load(Ordering::Relaxed)).collect()
    }

    fn strategy_sum_snapshot(&self) -> Vec<i32> {
        self.strategy_sum.iter().map(|r| r.load(Ordering::Relaxed)).collect()
    }

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
        strategy
    }

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
        strategy
    }

    pub fn add_regret(&self, i: usize, delta: i32) {
        let _ = self.regrets[i].fetch_update(Ordering::Relaxed, Ordering::Relaxed, |cur| {
            let next =
                (i64::from(cur) + i64::from(delta)).clamp(i32::MIN as i64, i32::MAX as i64);
            Some(next as i32)
        });
    }

    pub fn add_strategy_sum(&self, i: usize, delta: i32) {
        let _ = self.strategy_sum[i].fetch_update(Ordering::Relaxed, Ordering::Relaxed, |cur| {
            let next =
                (i64::from(cur) + i64::from(delta)).clamp(i32::MIN as i64, i32::MAX as i64);
            Some(next as i32)
        });
    }

    pub fn floor_regrets_at_zero(&self) {
        for r in self.regrets.iter() {
            r.fetch_max(0, Ordering::Relaxed);
        }
    }

    pub fn discount(&self, d: f32) {
        for r in self.regrets.iter() {
            let _ = r.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |cur| {
                Some((cur as f32 * d) as i32)
            });
        }
        for s in self.strategy_sum.iter() {
            let _ = s.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |cur| {
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

    #[test]
    fn sparse_row_allocates_on_first_write() {
        let mut row = InfosetRow::new();
        row.ensure_capacity(10, 3);
        assert_eq!(row.allocated_count(), 0);
        assert_eq!(row.final_strategy_or_uniform(0), vec![1.0 / 3.0; 3]);
        row.get_or_init(0).add_regret(0, 100);
        assert_eq!(row.allocated_count(), 1);
        assert_eq!(row.get_or_init(0).regret(0), 100);
        assert_eq!(row.allocated_count(), 1);
    }
}
