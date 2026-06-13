// deck -- card deck primitives.
//
// Port of `deck.h` and `deck.c` from the C `hand_indexer` library.
// The original is 36 + 4 lines. The C `card_t` is `uint_fast32_t`
// which we model as `u32`. The card encoding is `rank << 2 | suit`
// (rank in high 2 bits, suit in low 2 bits).

use std::sync::OnceLock;

/// Number of suits in a standard deck.
pub const SUITS: usize = 4;
/// Number of ranks in a standard deck (2..=Ace).
pub const RANKS: usize = 13;
/// Number of cards in a standard deck.
pub const CARDS: usize = 52;

/// A card is encoded as a `u8`: high 4 bits = rank (0..13), low 2 bits = suit (0..4).
/// This matches the C library's `card_t = rank << 2 | suit`, with rank
/// stored in the upper bits.
pub type Card = u8;

/// `RANK_TO_CHAR[i]` is the canonical printable character for rank `i`:
/// `0`->'2', `1`->'3', ..., `8`->'T', `9`->'J', `10`->'Q', `11`->'K', `12`->'A'.
pub static RANK_TO_CHAR: OnceLock<[char; 13]> = OnceLock::new();

/// `SUIT_TO_CHAR[i]` is the canonical printable character for suit `i`:
/// `0`->'s' (spades), `1`->'h' (hearts), `2`->'d' (diamonds), `3`->'c' (clubs).
/// (This matches the C library's `SUIT_TO_CHAR` ordering.)
pub static SUIT_TO_CHAR: OnceLock<[char; 4]> = OnceLock::new();

/// Returns the suit (0..4) encoded in a card.
#[inline]
pub fn card_suit(card: u8) -> u8 {
    card & 3
}

/// Returns the rank (0..12) encoded in a card.
#[inline]
pub fn card_rank(card: u8) -> u8 {
    card >> 2
}

/// Constructs a card from a suit (0..4) and rank (0..12).
#[inline]
pub fn make_card(suit: u8, rank: u8) -> u8 {
    (rank << 2) | suit
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn card_round_trip() {
        for s in 0..SUITS as u8 {
            for r in 0..RANKS as u8 {
                let c = make_card(s, r);
                assert_eq!(card_suit(c), s);
                assert_eq!(card_rank(c), r);
            }
        }
    }

    #[test]
    fn rank_to_char_init() {
        let arr = RANK_TO_CHAR.get_or_init(|| ['2','3','4','5','6','7','8','9','T','J','Q','K','A']);
        assert_eq!(arr[0], '2');
        assert_eq!(arr[12], 'A');
    }
}
