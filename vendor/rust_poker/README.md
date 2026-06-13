# vendor/rust_poker

A local fork of `rust_poker 0.1.5` (originally `kmurf1999/rust_poker`).
Used via `[patch.crates-io]` in the project's `Cargo.toml`.

## Why a fork?

The upstream crate is unmaintained and has three issues that block
running the solver on stable Rust with multiple threads:

1. **Nightly gate**: `rust_poker 0.1.5` declares
   `#![feature(test)]` at the crate root, which is rejected by stable
   Rust (`E0554`). Removed.

2. **Broken `lazy_static!`**: the crate uses `lazy_static!` 1.5.0 to
   initialize `CARDS` and `LOOKUP_TABLE`. That version of `lazy_static!`
   panics with "Once instance has previously been poisoned" when
   multiple threads first access the static concurrently. The MCCFR
   trainer spawns 8 worker threads; the panic is essentially guaranteed.
   Fixed by replacing the two `lazy_static!` blocks with
   `std::sync::OnceLock`, plus a public `init_cards()` / `init_lookup_table()`
   helper that lets `main()` force-initialize both tables on the main
   thread before workers spawn.

3. **Runtime `OUT_DIR`**: the `Evaluator::init()` reads `offset_table.dat`
   from `OUT_DIR`, which is a build-time-only env var. The fix in
   `src/solver/main.rs` sets `OUT_DIR` to the project's
   `target/release/deps/` directory at runtime if not already set.

4. **C `hand_indexer` library removed**: the original
   `rust_poker 0.1.5` builds the C `hand_indexer` library via
   `cmake` + `bindgen`, which requires cmake and libclang at
   build time and can OOM the linker on low-memory machines.
   The C source under `vendor/rust_poker/hand_indexer/` has been
   deleted; `build.rs` is now a no-op; and `src/hand_indexer.rs`
   is a pure-Rust stub with the same public API
   (`init`, `size`, `get_index`, `get_hand`) but `get_index`
   returns a suit-canonicalized `u64` hash instead of the C
   library's dense 0..size index. This lets the solver build
   and run without cmake/libclang. The full hand-indexing
   algorithm is being reimplemented in `crates/poker_canon/` at
   the parent repo; once that lands, this stub will be replaced.

## Files changed (vs upstream 0.1.5)

- `src/hand_evaluator/hand.rs`: removed `lazy_static!` for `CARDS`;
  added `init_cards()`; updated all `CARDS[i]` callers to use
  `CARDS.get().expect("CARDS")[i]`.
- `src/hand_evaluator/evaluator.rs`: removed `lazy_static!` for
  `LOOKUP_TABLE`; added `init_lookup_table()`; updated `evaluate()` to
  use the new API.
- `src/hand_evaluator/mod.rs`: re-exported `init_cards`,
  `init_lookup_table`, `LOOKUP_TABLE`.
- `src/equity_calculator/equity_calc.rs`: same `CARDS[i]` pattern
  (mechanical `sed` change).
- `src/lib.rs`: removed `#![feature(test)]` and `extern crate test`.
- `build.rs`: replaced C-binding generation with a no-op.
- `Cargo.toml`: removed `bindgen` and `cmake` build-deps.
- `hand_indexer/`: directory deleted (C source).
- `src/hand_indexer.rs`: replaced the C-binding FFI with a
  pure-Rust stub. `init` records the args, `size` returns
  hard-coded values for known configurations (12888, 54912,
  2598960), `get_index` returns a `DefaultHasher` hash of the
  sorted cards, `get_hand` writes a sentinel (0xff) to all bytes.

## Upstream

- Repo: <https://github.com/kmurf1999/rust_poker>
- Crates.io: <https://crates.io/crates/rust_poker/0.1.5>
- Patches in this fork are local; not pushed upstream (the original
  repo is unmaintained).
