extern crate bytepack;

mod hand;
mod evaluator;

pub use evaluator::{evaluate, init_lookup_table, LOOKUP_TABLE};
pub use hand::{Hand, CARDS, init_cards};
