use crate::actions::Action;
use crate::state::BettingRound;

#[derive(Debug)]
pub struct ActionNode {
    pub actions: Vec<Action>,
    pub index: usize,
    pub player: u8,
    pub round_idx: u8
}

/**
 * to be evaluated by cfr function
 * type: ALLIN, UNCONTESTED, SHOWDOWN
 */
#[derive(Debug)]
pub enum TerminalType {
    ALLIN,
    SHOWDOWN,
    UNCONTESTED
}

impl TerminalType {
    pub fn to_string(&self) -> String {
        return match self {
            TerminalType::ALLIN => String::from("ALLIN"),
            TerminalType::SHOWDOWN => String::from("SHOWDOWN"),
            TerminalType::UNCONTESTED => String::from("UNCONTESTED"),
        }
    }
}

#[derive(Debug)]
pub struct TerminalNode {
    pub value: u32, // size of pot (post-rake)
    pub ttype: TerminalType,
    pub last_to_act: u8, // 0 or 1
    pub round: BettingRound,
    /// Per-player wager at terminal state, in chip order.
    /// Used by the MCCFR walker to compute correct N-player
    /// payoffs (winner gets `sum_{j != i} min(wager_i, wager_j)`;
    /// losers lose their wager).
    pub player_wagers: Vec<u32>,
}

#[derive(Debug)]
pub struct PublicChanceNode {
    pub round: BettingRound
}

#[derive(Debug)]
pub enum GameTreeNode {
    Action(ActionNode),
    Terminal(TerminalNode),
    PublicChance(PublicChanceNode),
    PrivateChance,
}
