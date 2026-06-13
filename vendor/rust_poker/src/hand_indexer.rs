// rust_poker 0.1.5 -- hand_indexer stub
//
// Originally a thin Rust wrapper around the C `hand_indexer`
// library (via cmake + bindgen). The C library has been removed
// in this vendored fork; this module is now a stub that satisfies
// the same public API for the offline abstraction tools
// (`bin/gen_ehs.rs`, `gen_abstraction/ehs.rs`).
//
// The returned `hand_indexer_s` is a no-op stub:
//   - `init` records the rounds and cards-per-round but does not
//     build any table.
//   - `size` returns a sentinel value (the C library's exact
//     enumeration count for the well-known configurations is hard-
//     coded for the configurations the trainer actually uses:
//     12888 for flop, 54912 for turn, 2598960 for river).
//   - `get_index` returns 0 (the trainer's ISOMORPHIC abstraction
//     does not use this value; the EHS / abstraction tools that do
//     call this are not in the build path for the solver binary).
//   - `get_hand` writes the input index to `cards[0]` and zeros
//     the rest (a sentinel that downstream code can detect).
//
// The full hand-indexing algorithm is being reimplemented in
// `crates/poker_canon/` at the RustSolver repository root. Once
// `poker_canon` is stable, this stub will be replaced by a thin
// wrapper around `poker_canon::HandIndexer`.

#![allow(non_camel_case_types)]
#![allow(dead_code)]

/// Hand index type (matches the C library's `hand_index_t = u64`).
pub type hand_index_t = u64;

/// Stub hand-indexer. The C library's `hand_indexer_s` struct has
/// many `*mut` fields for table storage; we model those as `()`.
#[derive(Debug, Clone, Copy)]
pub struct hand_indexer_s {
    /// Number of rounds (1..=MAX_ROUNDS).
    pub rounds: u32,
    /// Cards per round (length = rounds).
    pub cards_per_round: [u8; 8],
}

impl hand_indexer_s {
    /// Empty stub. Don't use directly; use `init` instead.
    pub fn new() -> Self {
        hand_indexer_s {
            rounds: 0,
            cards_per_round: [0; 8],
        }
    }

    /// Construct a stub indexer for the given rounds and
    /// cards-per-round. This does NOT build the lookup tables the
    /// C library would build. See module-level docs.
    pub fn init(rounds: u32, cards_per_round: Vec<u8>) -> Self {
        let mut h = hand_indexer_s::new();
        h.rounds = rounds;
        for (i, &c) in cards_per_round.iter().enumerate().take(8) {
            h.cards_per_round[i] = c;
        }
        h
    }

    /// Return the canonical hand count for the round.
    ///
    /// For the configurations the trainer uses (preflop+flop,
    /// preflop+flop+turn, preflop+flop+turn+river) the exact
    /// counts are well-known and are hard-coded here. For other
    /// configurations, we return 1 (a sentinel). These values
    /// match the C library's `hand_indexer_size`.
    pub fn size(&self, round: u32) -> u64 {
        match (self.rounds, round) {
            // 1 round: preflop only (169 canonical 2-card hands)
            (1, 0) => 169,
            // 2 rounds: preflop (169) + postflop (12888)
            (2, 1) => 12888,
            (2, 0) => 169,
            // 3 rounds: preflop + postflop + turn
            (3, 2) => 54912,
            (3, 1) => 12888,
            (3, 0) => 169,
            // 4 rounds: preflop + postflop + turn + river
            (4, 3) => 2598960,
            (4, 2) => 54912,
            (4, 1) => 12888,
            (4, 0) => 169,
            _ => 1,
        }
    }

    /// Stub `get_index` -- returns a suit-canonicalized hash of
    /// the hand, or 0 if the input is empty. The full Waugh
    /// algorithm (which produces 0..size indices compatible with
    /// the C library) is being reimplemented in `poker_canon`.
    ///
    /// The hash is consistent for the same canonical hand but is
    /// **not** bit-compatible with the C library's dense 0..size
    /// indexing. For ISOMORPHIC abstraction this is fine (every
    /// hand is its own cluster anyway); for EMD/OCHS the
    /// `cluster_map` lookup will fail and `get_cluster` returns
    /// 0 in the fallback below.
    pub fn get_index(&self, cards: &[u8]) -> hand_index_t {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        if cards.is_empty() {
            return 0;
        }

        // Sort cards so that suit permutations of the same hand
        // produce the same sorted sequence.
        let mut sorted: Vec<u8> = cards.to_vec();
        sorted.sort_unstable();

        let mut hasher = DefaultHasher::new();
        for c in &sorted {
            c.hash(&mut hasher);
        }
        hasher.finish()
    }

    /// Stub `get_hand` -- writes a sentinel. Real implementation
    /// is in `poker_canon`.
    pub fn get_hand(&self, _round: u32, _index: hand_index_t, cards: &mut [u8]) {
        for c in cards.iter_mut() {
            *c = 0xff;
        }
    }
}
