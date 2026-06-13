# poker_canon

**Status: paused.** This crate was started as a pure-Rust replacement
for the C `hand_indexer` library that `rust_poker 0.1.5` depends on.
The C dependency has since been **eliminated by stubbing out the
hand_indexer** in the vendored `rust_poker`, so the build no longer
needs cmake or libclang. See `vendor/rust_poker/README.md`.

The full Waugh hand-indexing algorithm port remains a research
project; this directory holds the in-progress code (some
type-checking errors remain). When ready, this crate will replace
the stub and produce bit-for-bit compatible indices to the C
version.

The current state of the file:
- `src/deck.rs` -- the trivial card-deck primitives (ported)
- `src/hand_indexer.rs` -- the 589-line algorithm port; **does not
  compile** as of this writing. The main work remaining is:
  - Replace the closure-based recursion in `enumerate_configurations` /
    `enumerate_permutations` with explicit state-passing (the Rust
    borrow checker rejects the C-style `FnMut` over recursive calls).
  - Fix the type errors flagged by `cargo build` (u32 vs u64 mismatches,
    index types, etc).
  - Verify bit-for-bit compatibility with the C version by
    cross-checking `get_index` and `get_hand` outputs on
    preflop+flop, preflop+flop+turn, and preflop+flop+turn+river.

References:
- Kevin Waugh, "Hand Strength in Omaha Poker" (CMU MS Thesis, 2013).
  https://www.cs.cmu.edu/~waugh/papers/thesis.pdf
- The C source is in `vendor/rust_poker/hand_indexer/`
  (preserved in git history at commit `8adf9b6`).
