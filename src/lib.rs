//! Library surface for `rust_solver` (MCCFR postflop solver).
//!
//! Binaries (`solver`, `kk_turn_bench`) keep their own `mod` trees; this crate
//! root is the shared dependency for `rust_solver_py`.

#[path = "solver/constants.rs"]
pub mod constants;
#[path = "solver/state.rs"]
pub mod state;
#[path = "solver/tree.rs"]
pub mod tree;
#[path = "solver/nodes.rs"]
pub mod nodes;
#[path = "solver/actions.rs"]
pub mod actions;
#[path = "solver/options.rs"]
pub mod options;
#[path = "solver/tree_builder.rs"]
pub mod tree_builder;
#[path = "solver/card_abstraction.rs"]
pub mod card_abstraction;
#[path = "solver/infoset.rs"]
pub mod infoset;
#[path = "solver/range_parse.rs"]
pub mod range_parse;
#[path = "solver/cfr.rs"]
pub mod cfr;
#[path = "solver/python_api.rs"]
pub mod python_api;
#[path = "solver/benchmark/mod.rs"]
pub mod benchmark;
