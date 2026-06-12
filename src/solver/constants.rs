// Solver-wide constants. Most of these have migrated to `Options` and
// per-`GameState` fields. The remaining constants are used as
// fall-back defaults and for backwards compatibility with the 2p code
// path in `cfr.rs` and `tree_builder.rs`.

/// Maximum number of players supported by the trainer. The postflop
/// solver currently supports 2 and 3 players (2p code path is fully
/// tested; 3p code path is wired but not yet benchmarked).
pub const MAX_PLAYERS: usize = 3;

/// Number of players the trainer is configured for. Defaults to 2 for
/// backwards compatibility; 3 is the target for Phase 6.
pub const NUM_PLAYERS: usize = 2;

/// Default all-in threshold (fraction of remaining stack). Used when
/// `Options::all_in_threshold` is not otherwise specified.
pub const ALLIN_THRESHOLD: f64 = 0.67;

/// Default maximum raises per street. 2 -> max 3-bet; 3 -> max 4-bet.
/// Used when `Options::max_raises` is not otherwise specified.
pub const MAX_RAISES: u8 = 2;
