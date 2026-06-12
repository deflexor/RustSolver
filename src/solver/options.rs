/**
 * Post Flop Solver options.
 */

use rust_poker::hand_range::{HandRange, get_card_mask};
use crate::actions::ActionAbstraction;

#[derive(Debug, Clone)]
pub struct Options {
    /// Number of players. Must match `stack_sizes.len()`.
    pub n_players: usize,
    /// Hand range for each player. Loaded from `ranges/*.json` (Phase 8)
    /// or constructed in code. Length must equal `n_players`.
    pub hand_ranges: Vec<HandRange>,
    /// Starting stack for each player. Length must equal `n_players`.
    pub stack_sizes: Vec<u32>,
    /// Public card board as a 52-bit mask (bit i = card i).
    pub board_mask: u64,
    /// Initial size of the pot at the start of postflop play.
    pub starting_pot: u32,
    /// If a bet or raise target exceeds this fraction of the actor's
    /// stack, cap at the actor's remaining stack (all-in).
    pub all_in_threshold: f32,
    /// Maximum number of raises per street. 2 -> max 3-bet; 3 -> max 4-bet.
    pub max_raises: u8,
    /// Per-street action abstraction. Outer index is the round
    /// (0=flop, 1=turn, 2=river).
    pub action_abstraction: ActionAbstraction,
    /// Depth tier in big blinds. Discrete values {5, 8, 10, 12, 15, 18,
    /// 20, 25}. One solver is trained per tier.
    pub depth_tier_bb: u32,
    /// Optional override for `starting_pot`. When set, `starting_pot` is
    /// ignored. Used for limped pots (e.g. 4BB limped = 8) where the
    /// default 1.5BB doesn't apply.
    pub postflop_pot_override: Option<u32>,
    /// Rake as (fraction, cap_in_chips). `None` means no rake.
    pub rake: Option<(f64, u32)>,
    /// Maximum number of distinct action sequences to allow per street.
    /// Beyond this cap, tree_builder prunes leaves to the closest
    /// representative. Default 200. Used to bound tree width in 3p.
    pub max_action_sequences_per_street: u32,
}

impl Options {
    /// Create an Options struct for a single depth tier with the given
    /// number of players and preflop ranges. This is the canonical
    /// factory used by `solve_tier` (Phase 7) and by the smoke tests.
    pub fn for_tier(
        n_players: usize,
        depth_tier_bb: u32,
        board_mask: u64,
        starting_pot: u32,
        hand_ranges: Vec<HandRange>,
    ) -> Self {
        assert!(
            (2..=3).contains(&n_players),
            "n_players must be 2 or 3"
        );
        assert_eq!(
            hand_ranges.len(),
            n_players,
            "hand_ranges.len() must equal n_players"
        );
        let stack = depth_tier_bb * 100; // 1 BB = 100 chips by convention
        Options {
            n_players,
            stack_sizes: vec![stack; n_players],
            hand_ranges,
            board_mask,
            starting_pot,
            all_in_threshold: 0.67,
            max_raises: 2,
            action_abstraction: ActionAbstraction {
                bet_sizes: vec![vec![0.5, 1.0]],
                raise_sizes: vec![vec![3.0]],
            },
            depth_tier_bb,
            postflop_pot_override: None,
            rake: None,
            max_action_sequences_per_street: 200,
        }
    }
}

pub fn default_flop() -> Options {
    Options {
        n_players: 2,
        stack_sizes: vec![500, 500],
        board_mask: get_card_mask("4d5dAs3cKs"),
        starting_pot: 35,
        all_in_threshold: 0.67,
        max_raises: 2,
        hand_ranges: vec![
            HandRange::from_string("random".to_string()),
            HandRange::from_string("random".to_string()),
        ],
        action_abstraction: ActionAbstraction {
            bet_sizes: vec![
                vec![0.5, 1.0],
            ],
            raise_sizes: vec![
                vec![3.0],
            ],
        },
        depth_tier_bb: 5,
        postflop_pot_override: None,
        rake: None,
        max_action_sequences_per_street: 200,
    }
}
