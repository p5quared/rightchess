use std::fmt;
use std::str::FromStr;

use anyhow::{Result, anyhow, bail, ensure};

const STARTING: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

/// One position in FEN as written, e.g. `rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq
/// e3 0 1`. Only well-formed text is a `Fen`: six fields, eight ranks of eight squares, castling
/// rights in `KQkq` order, an en passant square behind the side that just moved, and counters
/// that fit a `u32`. Whether the position could arise isn't checked.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Fen(pub(crate) String);

impl Fen {
    pub fn starting() -> Fen {
        str::parse(STARTING).expect("STARTING FEN should parse")
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for Fen {
    type Err = anyhow::Error;

    fn from_str(text: &str) -> Result<Self> {
        check(text).map_err(|reason| anyhow!("\"{text}\" is not a FEN: {reason}"))?;
        Ok(Fen(text.to_string()))
    }
}

impl fmt::Display for Fen {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Fails with the reason `text` isn't a FEN.
fn check(text: &str) -> Result<()> {
    let fields: Vec<&str> = text.split(' ').collect();
    let [board, side, castling, en_passant, halfmove, fullmove] = fields[..] else {
        bail!("it needs 6 fields separated by single spaces");
    };
    let ranks: Vec<&str> = board.split('/').collect();
    ensure!(
        ranks.len() == 8,
        "the board has {} ranks, not 8",
        ranks.len()
    );
    for rank in ranks {
        ensure!(rank_len(rank) == Some(8), "rank {rank:?} isn't 8 squares");
    }
    let en_passant_rank = match side {
        "w" => '6',
        "b" => '3',
        _ => bail!("the side to move is {side:?}, not w or b"),
    };
    ensure!(
        is_castling(castling),
        "castling rights {castling:?} aren't - or KQkq in order"
    );
    ensure!(
        en_passant == "-" || is_square_on(en_passant, en_passant_rank),
        "the en passant square {en_passant:?} isn't - or on rank {en_passant_rank}"
    );
    counter(halfmove)?;
    ensure!(counter(fullmove)? > 0, "the fullmove number is 0");
    Ok(())
}

/// The squares a rank of the board spans, or None if it isn't pieces and digits for runs of
/// empty squares, never two digits in a row.
fn rank_len(rank: &str) -> Option<u32> {
    let mut len = 0;
    let mut after_digit = false;
    for c in rank.chars() {
        match c {
            '1'..='8' if !after_digit => len += c.to_digit(10)?,
            'p' | 'n' | 'b' | 'r' | 'q' | 'k' | 'P' | 'N' | 'B' | 'R' | 'Q' | 'K' => len += 1,
            _ => return None,
        }
        after_digit = c.is_ascii_digit();
    }
    Some(len)
}

/// Whether `rights` is `-`, or some of `KQkq` in that order.
fn is_castling(rights: &str) -> bool {
    let mut rest = "KQkq";
    let in_order = rights.chars().all(|c| match rest.find(c) {
        Some(i) => {
            rest = &rest[i + 1..];
            true
        }
        None => false,
    });
    rights == "-" || (!rights.is_empty() && in_order)
}

fn is_square_on(square: &str, rank: char) -> bool {
    matches!(square.as_bytes(), [b'a'..=b'h', r] if *r as char == rank)
}

/// A counter: digits without leading zeros that fit a `u32`.
fn counter(text: &str) -> Result<u32> {
    let digits = !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit());
    ensure!(
        digits && (text == "0" || !text.starts_with('0')),
        "counter {text:?} isn't a number"
    );
    text.parse()
        .map_err(|_| anyhow!("counter {text} doesn't fit a u32"))
}
