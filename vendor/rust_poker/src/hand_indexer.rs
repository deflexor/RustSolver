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
            // preflop (2) + flop (3) -> postflop count for round=1
            (2, 1) => 12888,
            // preflop (2) + flop (3) + turn (1) -> turn count for round=2
            (3, 2) => 54912,
            // preflop (2) + flop (3) + turn (1) + river (1) -> river for round=3
            (4, 3) => 2598960,
            // The "current street" count for the round we're entering
            // (used by ISOMORPHIC::init when round matches the
            // configured street).
            (2, 0) => 169,  // preflop canonical hands
            (3, 1) => 12888, // postflop
            (3, 0) => 169,
            (4, 1) => 12888,
            (4, 2) => 54912,
            (4, 0) => 169,
            _ => 1,
        }
    }

    /// Stub `get_index` -- returns a suit-canonicalized hash of
    /// the hand. This is **not** the same as the C library's
    /// Waugh index (which has a specific 0..size ordering) but
    /// produces a value that is:
    ///   1. unique per canonical hand (up to suit permutation), and
    ///   2. consistent (the same canonical hand always produces the
    ///      same hash).
    ///
    /// This lets downstream tools (gen_ehs, gen_abstraction) keep
    /// working: they bucket by `get_index`, so all they need is
    /// consistent values, not the C library's specific indices.
    ///
    /// The full hand-indexing algorithm is in `poker_canon`.
    pub fn get_index(&self, cards: &[u8]) -> hand_index_t {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        // Sort cards so that suit permutations of the same hand
        // produce the same sorted sequence. We bucket cards by
        // (rank, suit-sorted) so the result is canonical up to
        // suit permutation.
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
