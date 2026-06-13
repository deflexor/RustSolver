// poker_canon -- Pure-Rust port of Kevin Waugh's 2013 hand indexing
// algorithm.
//
// This crate is a drop-in replacement for the C `hand_indexer` library
// that ships with `rust_poker 0.1.5`. The C library requires cmake +
// bindgen + libclang to build; this crate is pure Rust and is
// publishable on crates.io.
//
// The algorithm is described in Kevin Waugh's 2013 CMU MS thesis
// "Hand Strength in Omaha Poker". The C source is preserved in
// `vendor/rust_poker/hand_indexer/` of the parent repository for
// reference.
//
// Public API:
//
//   use poker_canon::HandIndexer;
//   let idx = HandIndexer::init(2, &[2, 3]).unwrap();
//   let n = idx.size(1);
//   let i = idx.get_index(&[0, 1, 5, 6, 7]);
//   let mut out = [0u8; 5];
//   idx.get_hand(1, i, &mut out);

#![deny(missing_docs)]
#![deny(unsafe_op_in_unsafe_fn)]

mod deck;
mod hand_indexer;

pub use hand_indexer::{HandIndex, HandIndexer};
pub use deck::{SUITS, RANKS, CARDS};

/// Re-exported for compatibility with the C library's
/// `hand_indexer.h` constants. The C library uses `MAX_ROUNDS = 8`
/// (it supports up to 8 streets of cards).
pub const MAX_ROUNDS: usize = 8;
