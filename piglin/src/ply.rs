use std::fmt;
use std::str::FromStr;

use anyhow::{Result, ensure};

/// One half-move in SAN as written, e.g. `Nf3+`. Only text that is SAN can be a `Ply`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Ply(String);

impl Ply {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for Ply {
    type Err = anyhow::Error;

    fn from_str(text: &str) -> Result<Self> {
        ensure!(is_san(text.as_bytes()), "{text:?} is not a SAN move");
        Ok(Ply(text.to_string()))
    }
}

/// Whether `text` is a move, castling, drop or null move, with an optional `+` or `#` after all
/// but the null move.
fn is_san(text: &[u8]) -> bool {
    let body = text
        .strip_suffix(b"+")
        .or(text.strip_suffix(b"#"))
        .unwrap_or(text);
    text == b"--" || is_castling(body) || is_drop(body) || is_piece_move(body) || is_pawn_move(body)
}

fn is_castling(body: &[u8]) -> bool {
    [&b"O-O"[..], b"O-O-O", b"0-0", b"0-0-0"].contains(&body)
}

/// A drop such as `N@f3`. Pawns can't be dropped on the first or last rank.
fn is_drop(body: &[u8]) -> bool {
    match *body {
        [b'P', b'@', file, rank] => is_file(file) && is_pawn_rank(rank, false),
        [b'N' | b'B' | b'R' | b'Q', b'@', file, rank] => is_square(file, rank),
        _ => false,
    }
}

/// A piece move such as `Nf3`, `Nbd7`, `R1e2` or `Qh4xe1`.
fn is_piece_move(body: &[u8]) -> bool {
    let [b'K' | b'Q' | b'R' | b'B' | b'N', ref rest @ ..] = *body else {
        return false;
    };
    let Some((from, &[file, rank])) = rest.split_last_chunk() else {
        return false;
    };
    let from = from.strip_suffix(b"x").unwrap_or(from);
    let from_ok = match *from {
        [] => true,
        [c] => is_file(c) || is_rank(c),
        [file, rank] => is_square(file, rank),
        _ => false,
    };
    from_ok && is_square(file, rank)
}

/// A pawn move such as `e4`, `exd5` or `e8=Q`. A pawn captures onto a neighbouring file, and
/// promotes exactly when it reaches the first or last rank.
fn is_pawn_move(body: &[u8]) -> bool {
    let (body, promotes) = match *body {
        [ref rest @ .., b'=', b'Q' | b'R' | b'B' | b'N' | b'K'] => (rest, true),
        _ => (body, false),
    };
    match *body {
        [file, rank] => is_file(file) && is_pawn_rank(rank, promotes),
        [from, b'x', file, rank] => {
            is_file(from)
                && is_file(file)
                && from.abs_diff(file) == 1
                && is_pawn_rank(rank, promotes)
        }
        _ => false,
    }
}

/// Whether a pawn may arrive on `rank`: the first or last rank if it promotes, any other if not.
fn is_pawn_rank(rank: u8, promotes: bool) -> bool {
    is_rank(rank) && matches!(rank, b'1' | b'8') == promotes
}

fn is_square(file: u8, rank: u8) -> bool {
    is_file(file) && is_rank(rank)
}

fn is_file(c: u8) -> bool {
    matches!(c, b'a'..=b'h')
}

fn is_rank(c: u8) -> bool {
    matches!(c, b'1'..=b'8')
}

impl fmt::Display for Ply {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
