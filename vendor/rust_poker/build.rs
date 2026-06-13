// build.rs (rust_poker 0.1.5, vendored fork)
//
// Originally generated FFI bindings to the C `hand_indexer` library
// via cmake + bindgen. The C library has been removed; this build
// script is now a no-op. The Rust hand_indexer module in
// `src/hand_indexer.rs` is a pure-Rust stub that satisfies the same
// API surface for the offline abstraction tools
// (`bin/gen_ehs.rs`, `gen_abstraction/ehs.rs`).
//
// The full hand-indexing algorithm is being reimplemented in the
// `poker_canon` crate at the RustSolver repository root. Once that
// crate is bit-for-bit compatible with the C version, this stub
// will be replaced by a thin wrapper around `poker_canon::HandIndexer`.

fn main() {
    // No-op. The C `hand_indexer` library is no longer built.
    println!("cargo:rerun-if-changed=build.rs");
}
