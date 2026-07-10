use crate::actions::{ActionAbstraction, Action};
use crate::options::Options;


#[derive(PartialEq, Eq, Debug, Copy, Clone)]
pub enum BettingRound {
    Flop,
    Turn,
    River,
}

impl BettingRound {
    pub fn to_usize(&self) -> usize {
        return match self {
            BettingRound::Flop => 0,
            BettingRound::Turn => 1,
            BettingRound::River => 2,
        };
    }

    /// Returns the next round in postflop order, panicking at River.
    pub fn next(self) -> BettingRound {
        match self {
            BettingRound::Flop => BettingRound::Turn,
            BettingRound::Turn => BettingRound::River,
            BettingRound::River => panic!("BettingRound::next called on River"),
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub struct PlayerState {
    pub stack: u32,
    pub wager: u32,
    pub has_folded: bool,
    pub has_acted_this_street: bool,
}

impl PlayerState {
    pub fn init(stack: u32) -> PlayerState {
        PlayerState {
            stack,
            wager: 0,
            has_folded: false,
            has_acted_this_street: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct GameState {
    pub players: Vec<PlayerState>,
    pub pot: u32,
    pub raise_count: u8,
    pub current: u8,
    pub round: BettingRound,
    pub bets_settled: bool,
    pub max_raises: u8,
    pub all_in_threshold: f32,
}

impl GameState {
    pub fn num_players(&self) -> usize {
        self.players.len()
    }

    pub fn current_player(&self) -> &PlayerState {
        &self.players[usize::from(self.current)]
    }

    pub fn current_player_mut(&mut self) -> &mut PlayerState {
        &mut self.players[usize::from(self.current)]
    }

    /// Index of the next player who can act, skipping folded players.
    /// If all other players are folded, returns `self.current` (caller
    /// should check `is_uncontested`).
    pub fn next_active_player(&self) -> u8 {
        let n = self.num_players();
        for offset in 1..=n {
            let candidate = (usize::from(self.current) + offset) % n;
            if !self.players[candidate].has_folded && self.players[candidate].stack > 0 {
                return candidate as u8;
            }
        }
        // All opponents are folded or all-in. Caller decides what to do.
        self.current
    }

    /// Highest wager among active (non-folded) players.
    pub fn highest_wager(&self) -> u32 {
        self.players
            .iter()
            .filter(|p| !p.has_folded)
            .map(|p| p.wager)
            .max()
            .unwrap_or(0)
    }

    /// True if exactly one active (non-folded) player remains.
    pub fn is_uncontested(&self) -> bool {
        let active: usize = self.players.iter().filter(|p| !p.has_folded).count();
        active <= 1
    }

    /// True if at least one player is all-in.
    pub fn is_allin(&self) -> bool {
        self.players
            .iter()
            .any(|p| !p.has_folded && p.stack == 0)
    }

    pub fn is_terminal(&self) -> bool {
        (self.round == BettingRound::River && self.bets_settled)
            || self.is_allin()
            || self.is_uncontested()
    }

    /// True if the current betting street is closed. The street closes
    /// when every active player has matched the highest wager AND the
    /// player whose turn it is has already acted this street (meaning
    /// action has wrapped around from a bet/raise, or everyone has
    /// checked in sequence).
    pub fn street_closed(&self) -> bool {
        if self.bets_settled {
            return true;
        }
        // HU (or N-way): if at most one player can still put chips in,
        // no further betting is possible (e.g. short all-in call).
        let can_act = self
            .players
            .iter()
            .filter(|p| !p.has_folded && p.stack > 0)
            .count();
        if can_act <= 1 {
            return true;
        }
        let target = self.highest_wager();
        let all_matched = self
            .players
            .iter()
            .filter(|p| !p.has_folded)
            .all(|p| p.wager == target);
        all_matched && self.players[usize::from(self.current)].has_acted_this_street
    }

    /// Advance to the next street: reset wagers, reset has_acted, and
    /// return a new state. `to_next_street` does NOT advance the round;
    /// the caller (tree_builder) does that.
    pub fn to_next_street(&self) -> GameState {
        let mut new_state = self.clone();
        new_state.bets_settled = false;
        new_state.current = 0;
        new_state.raise_count = 0;
        for p in new_state.players.iter_mut() {
            p.wager = 0;
            p.has_acted_this_street = false;
        }
        new_state.round = self.round.next();
        new_state
    }

    /// N-player legal action generation. The `Action` enum's `Bet(f64)`
    /// and `Raise(f64)` are expressed as multipliers of pot and opponent
    /// wager respectively. When a bet/raise target exceeds
    /// `all_in_threshold * current_stack`, we cap the action and emit a
    /// single representative (the largest one that doesn't go over) so
    /// the action set remains bounded in 3p trees.
    pub fn valid_actions(&self, action_abs: &ActionAbstraction, round_idx: usize) -> Vec<Action> {
        let mut actions: Vec<Action> = Vec::new();
        let me = self.current_player();
        let target_wager = self.highest_wager();
        let can_check = target_wager == me.wager;
        let facing_bet = target_wager > me.wager;

        if can_check {
            actions.push(Action::Check);
        }
        if facing_bet {
            actions.push(Action::Call);
            actions.push(Action::Fold);
        }
        if can_check {
            // Bet sizes only valid when no one has bet yet this street.
            for bet_size in &action_abs.bet_sizes[round_idx] {
                let chips = bet_size * self.pot as f64;
                actions.push(Action::Bet(*bet_size));
                if chips > (self.all_in_threshold as f64) * me.stack as f64 {
                    break;
                }
            }
        }
        if facing_bet
            && self.raise_count < self.max_raises
            && !self.is_allin()
        {
            for raise_size in &action_abs.raise_sizes[round_idx] {
                let target = raise_size * target_wager as f64;
                actions.push(Action::Raise(*raise_size));
                if target > (self.all_in_threshold as f64) * me.stack as f64 {
                    break;
                }
            }
        }
        actions
    }

    /// N-player `apply_action` with the stack-cap side-pot convention.
    /// The actor's wager is capped at their own remaining stack, and
    /// any chips pulled from the actor to match a higher wager are
    /// added to the pot. Side pots are NOT tracked; the cap-by-stack
    /// convention suffices for the stack-cap-only solver model.
    pub fn apply_action(&self, action: &Action) -> GameState {
        let mut new_state = self.clone();
        let me_idx = usize::from(new_state.current);
        let target_wager = new_state.highest_wager();

        match action {
            Action::Bet(amt) => {
                let mut chips = (new_state.pot as f64 * amt) as u32;
                let cap = ((new_state.all_in_threshold as f64)
                    * new_state.players[me_idx].stack as f64) as u32;
                if chips > cap {
                    chips = new_state.players[me_idx].stack;
                }
                new_state.players[me_idx].stack -= chips;
                new_state.players[me_idx].wager = chips;
                new_state.pot += chips;
            }
            Action::Raise(amt) => {
                let target = (target_wager as f64 * amt) as u32;
                let mut chips = target.saturating_sub(new_state.players[me_idx].wager);
                let cap = ((new_state.all_in_threshold as f64)
                    * new_state.players[me_idx].stack as f64) as u32;
                if chips > cap {
                    chips = new_state.players[me_idx].stack;
                }
                new_state.players[me_idx].stack -= chips;
                new_state.players[me_idx].wager += chips;
                new_state.pot += chips;
                new_state.raise_count += 1;
            }
            Action::Call => {
                // Match the highest wager among active players, capped
                // at the actor's remaining stack.
                let wager_diff = target_wager.saturating_sub(new_state.players[me_idx].wager);
                let chips = wager_diff.min(new_state.players[me_idx].stack);
                new_state.players[me_idx].stack -= chips;
                new_state.players[me_idx].wager += chips;
                new_state.pot += chips;
            }
            Action::Check => {
                // No chips move; just advance.
            }
            Action::Fold => {
                new_state.players[me_idx].has_folded = true;
            }
        }

        new_state.players[me_idx].has_acted_this_street = true;
        new_state.current = new_state.next_active_player();
        if new_state.street_closed() {
            new_state.bets_settled = true;
        }
        new_state
    }
}

impl From<&Options> for GameState {
    fn from(options: &Options) -> Self {
        let players: Vec<PlayerState> = options
            .stack_sizes
            .iter()
            .map(|s| PlayerState::init(*s))
            .collect();
        let round = match options.board_mask.count_ones() {
            3 => BettingRound::Flop,
            4 => BettingRound::Turn,
            5 => BettingRound::River,
            _ => panic!(
                "invalid board mask ({} cards set); expected 3 (flop), 4 (turn), or 5 (river)",
                options.board_mask.count_ones()
            ),
        };
        GameState {
            players,
            round,
            current: 0,
            bets_settled: false,
            pot: options.postflop_pot_override.unwrap_or(options.starting_pot),
            raise_count: 0,
            max_raises: options.max_raises,
            all_in_threshold: options.all_in_threshold,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_2p(stacks: [u32; 2]) -> GameState {
        let options = crate::options::Options {
            n_players: 2,
            stack_sizes: stacks.to_vec(),
            hand_ranges: vec![],
            board_mask: 0b111,
            starting_pot: 6,
            all_in_threshold: 0.67,
            max_raises: 2,
            action_abstraction: crate::actions::ActionAbstraction {
                bet_sizes: vec![vec![0.5, 1.0]],
                raise_sizes: vec![vec![3.0]],
            },
            depth_tier_bb: 5,
            postflop_pot_override: None,
            rake: None,
            max_action_sequences_per_street: 200,
            preflop_ranges: None,
        };
        GameState::from(&options)
    }

    fn make_3p(stacks: [u32; 3]) -> GameState {
        let options = crate::options::Options {
            n_players: 3,
            stack_sizes: stacks.to_vec(),
            hand_ranges: vec![],
            board_mask: 0b111,
            starting_pot: 9,
            all_in_threshold: 0.67,
            max_raises: 2,
            action_abstraction: crate::actions::ActionAbstraction {
                bet_sizes: vec![vec![0.5, 1.0]],
                raise_sizes: vec![vec![3.0]],
            },
            depth_tier_bb: 5,
            postflop_pot_override: None,
            rake: None,
            max_action_sequences_per_street: 200,
            preflop_ranges: None,
        };
        GameState::from(&options)
    }

    #[test]
    fn next_active_player_wraps_in_2p() {
        let s = make_2p([100, 100]);
        // current=0; next active should be 1.
        assert_eq!(s.next_active_player(), 1);
        let s1 = s.apply_action(&Action::Check);
        // After p0 checks, current advances to 1.
        assert_eq!(s1.current, 1);
        assert_eq!(s1.next_active_player(), 0);
    }

    #[test]
    fn next_active_player_skips_folded_3p() {
        let mut s = make_3p([100, 100, 100]);
        // 0 checks
        let s1 = s.apply_action(&Action::Check);
        // current=1
        assert_eq!(s1.current, 1);
        // 1 folds
        let s2 = s1.apply_action(&Action::Fold);
        // current should skip 1 (folded) and go to 2
        assert_eq!(s2.current, 2);
        assert!(s2.players[1].has_folded);
    }

    #[test]
    fn call_caps_at_stack() {
        // 2p, p0 stack=5, p1 already wagered 20.
        let mut s = make_2p([5, 100]);
        s.players[1].wager = 20;
        s.players[1].has_acted_this_street = true;
        // p0's turn: must call to 20 but only has 5 chips.
        let after = s.apply_action(&Action::Call);
        assert_eq!(after.players[0].stack, 0);
        assert_eq!(after.players[0].wager, 5);
        // pot was 6, +5 = 11
        assert_eq!(after.pot, 11);
    }

    #[test]
    fn uncontested_3p_when_two_fold() {
        // 3p, p0 bets, p1 folds, p2 folds -> 1 active, uncontested.
        let mut s = make_3p([100, 100, 100]);
        // Manually set a non-zero wager to make the bet action meaningful.
        // Actually Bet(0.5) of pot 9 = 4 chips, so p0 will have wager 4.
        let s = s.apply_action(&Action::Bet(0.5));
        let s = s.apply_action(&Action::Fold); // p1 folds
        let s = s.apply_action(&Action::Fold); // p2 folds
        assert!(s.is_uncontested(), "after p1 and p2 fold, 1 player remains");
        assert!(!s.players[0].has_folded);
        assert!(s.players[1].has_folded);
        assert!(s.players[2].has_folded);
    }

    #[test]
    fn street_closed_after_match_2p() {
        // Start: p0 has bet (wager=10), p1 to call.
        let mut s = make_2p([100, 100]);
        s.players[0].wager = 10;
        s.players[0].has_acted_this_street = true; // p0 bet this street
        s.current = 1;
        let after = s.apply_action(&Action::Call);
        assert!(after.bets_settled, "street should be closed after call match");
        assert_eq!(after.players[1].wager, 10);
    }

    #[test]
    fn next_round_advances_round_idx() {
        let s = make_2p([100, 100]);
        assert_eq!(s.round, BettingRound::Flop);
        let s2 = s.to_next_street();
        assert_eq!(s2.round, BettingRound::Turn);
        let s3 = s2.to_next_street();
        assert_eq!(s3.round, BettingRound::River);
    }

    // ---- P1.7: 2p regression tests ----
    // Each test exercises `apply_action` through a specific hand
    // sequence and asserts exact pot, wager, and stack values.

    #[test]
    fn regr_check_check() {
        // Both check. Pot unchanged.
        let s = make_2p([100, 100]);
        assert_eq!(s.pot, 6);
        let s1 = s.apply_action(&Action::Check);
        assert_eq!(s1.current, 1);
        assert!(!s1.bets_settled);
        let s2 = s1.apply_action(&Action::Check);
        assert!(s2.bets_settled);
        assert_eq!(s2.pot, 6);
        assert_eq!(s2.players[0].wager, 0);
        assert_eq!(s2.players[1].wager, 0);
    }

    #[test]
    fn regr_bet_call() {
        // p0 bets half-pot (3 chips from pot 6), p1 calls.
        let s = make_2p([100, 100]);
        let s1 = s.apply_action(&Action::Bet(0.5));
        // pot 6, bet 0.5 => 3 chips wagered
        assert_eq!(s1.players[0].wager, 3);
        assert_eq!(s1.players[0].stack, 97);
        assert_eq!(s1.pot, 9);
        assert_eq!(s1.current, 1);
        let s2 = s1.apply_action(&Action::Call);
        assert!(s2.bets_settled);
        assert_eq!(s2.players[1].wager, 3);
        assert_eq!(s2.players[1].stack, 97);
        assert_eq!(s2.pot, 12);
    }

    #[test]
    fn regr_bet_raise_call() {
        // p0 bets 3, p1 raises to 9 (3x current wager), p0 calls.
        let s = make_2p([100, 100]);
        let s1 = s.apply_action(&Action::Bet(0.5));
        assert_eq!(s1.pot, 9);
        // p1 raises: raise_size * opp_wager = 3.0 * 3 = 9
        let s2 = s1.apply_action(&Action::Raise(3.0));
        assert_eq!(s2.players[1].wager, 9);
        assert_eq!(s2.players[1].stack, 91);
        assert_eq!(s2.pot, 18);
        assert_eq!(s2.current, 0); // back to p0
        let s3 = s2.apply_action(&Action::Call);
        assert!(s3.bets_settled);
        assert_eq!(s3.players[0].wager, 9);
        assert_eq!(s3.players[0].stack, 91);
        assert_eq!(s3.pot, 24);
    }

    #[test]
    fn regr_bet_fold_uncontested() {
        // p0 bets, p1 folds -> uncontested, p0 wins the pot.
        let s = make_2p([100, 100]);
        let s1 = s.apply_action(&Action::Bet(0.5));
        assert_eq!(s1.pot, 9);
        let s2 = s1.apply_action(&Action::Fold);
        assert!(s2.is_uncontested());
        assert_eq!(s2.players[1].has_folded, true);
        // p0's wager stays, p1 folded.
        assert_eq!(s2.players[0].wager, 3);
        assert_eq!(s2.pot, 9);
    }

    #[test]
    fn regr_stack_cap_on_call() {
        // p0 has only 5 chips, p1 has bet 20.
        // p0 calls, should be capped at their stack of 5.
        let mut s = make_2p([5, 100]);
        s.players[1].wager = 20;
        s.players[1].has_acted_this_street = true;
        s.pot = 26; // initial 6 + 20 from p1
        let after = s.apply_action(&Action::Call);
        assert_eq!(after.players[0].stack, 0, "p0 should be all-in");
        assert_eq!(after.players[0].wager, 5, "p0 caps at remaining stack");
        assert_eq!(after.pot, 31); // 26 + 5
    }
}
