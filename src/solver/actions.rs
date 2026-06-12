use crate::state::GameState;

#[derive(Debug, Copy, Clone)]
pub enum Action {
    Bet(f64),
    Raise(f64),
    Check,
    Call,
    Fold,
}

impl Action {
    pub fn to_string(&self) -> String {
        return match self {
            Action::Check => String::from("Check"),
            Action::Bet(amt) => format!("Bet {}", amt),
            Action::Raise(amt) => format!("Raise {}", amt),
            Action::Fold => String::from("Fold"),
            Action::Call => String::from("Call"),
        };
    }
}

#[derive(Debug)]
pub struct ActionAbstraction {
    pub bet_sizes: Vec<Vec<f64>>,
    pub raise_sizes: Vec<Vec<f64>>,
}
