use std::fmt;
use std::iter;
use std::ops::ControlFlow;

use anyhow::{Context, Result, anyhow, ensure};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SourceKind {
    ChessCom,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GameId {
    pub source: SourceKind,
    pub external_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pgn(pub String);

/// One half-move in SAN as written in the PGN, e.g. `Nf3+`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ply(pub String);

impl Pgn {
    /// The mainline's plies, in order. Each is a move as written minus trailing glyphs; tags,
    /// comments, variations, NAGs, move numbers and the result are skipped. Moves aren't checked
    /// for legality. Fails on an unterminated comment, tag or variation, or an unmatched `)`,
    /// `}` or `]`.
    pub fn plies(&self) -> Result<Vec<Ply>> {
        Ok(mainline(lexemes(&self.0))?
            .into_iter()
            .filter_map(move_text)
            .map(|san| Ply(san.to_string()))
            .collect())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Game {
    pub id: GameId,
    pub pgn: Pgn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct YearMonth {
    pub year: u16,
    pub month: u8,
}

impl fmt::Display for YearMonth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}", self.year, self.month)
    }
}

/// The lexemes outside variations.
fn mainline<'a>(mut lexemes: impl Iterator<Item = Result<&'a str>>) -> Result<Vec<&'a str>> {
    let (depth, kept) = lexemes.try_fold(
        (0u32, Vec::new()),
        |(depth, mut kept), lexeme| -> Result<_> {
            let depth = match lexeme? {
                "(" => depth + 1,
                ")" => depth.checked_sub(1).context("unmatched )")?,
                lexeme => {
                    if depth == 0 {
                        kept.push(lexeme);
                    }
                    depth
                }
            };
            Ok((depth, kept))
        },
    )?;
    ensure!(depth == 0, "unterminated variation");
    Ok(kept)
}

/// The comments, tags, NAGs, parentheses and words of `text`, ending at the first error.
fn lexemes(text: &str) -> impl Iterator<Item = Result<&str>> {
    iter::successors(next_lexeme(text), |step| next_lexeme(step.as_ref().ok()?.1))
        .map(|step| step.map(|(lexeme, _)| lexeme))
}

/// Splits `text`, after any whitespace, into its first lexeme and the rest, or None if only
/// whitespace is left.
fn next_lexeme(text: &str) -> Option<Result<(&str, &str)>> {
    let text = text.trim_start();
    let len = match text.chars().next()? {
        '{' => text
            .find('}')
            .map(|end| end + 1)
            .context("unterminated comment"),
        ';' => Ok(text.find('\n').unwrap_or(text.len())),
        '[' => tag_len(text),
        '(' | ')' => Ok(1),
        c @ ('}' | ']') => Err(anyhow!("unmatched {c}")),
        // A word, or a NAG such as `$14`, runs to the next delimiter.
        _ => Ok(text
            .char_indices()
            .skip(1)
            .find(|&(_, c)| c.is_whitespace() || "[]{}();$".contains(c))
            .map_or(text.len(), |(end, _)| end)),
    };
    Some(len.map(|len| text.split_at(len)))
}

/// The length of the tag pair `text` starts with, such as `[Event "Live Chess"]`. Its quoted
/// value may hold `]`, `\"` and `\\`.
fn tag_len(text: &str) -> Result<usize> {
    /// Where a character is: in the tag outside its value, in the value, or after a backslash.
    enum In {
        Tag,
        Value,
        Escape,
    }
    text.char_indices()
        .try_fold(In::Tag, |state, (i, c)| match (state, c) {
            (In::Tag, ']') => ControlFlow::Break(i + 1),
            (In::Tag, '"') | (In::Escape, _) => ControlFlow::Continue(In::Value),
            (In::Value, '"') => ControlFlow::Continue(In::Tag),
            (In::Value, '\\') => ControlFlow::Continue(In::Escape),
            (state, _) => ControlFlow::Continue(state),
        })
        .break_value()
        .context("unterminated tag")
}

/// The move in a lexeme without its move number or glyphs, or None if the lexeme holds no move:
/// a comment, tag, NAG, move number, glyph or result.
fn move_text(lexeme: &str) -> Option<&str> {
    let token = lexeme.trim_end_matches(['!', '?']);
    // A move number is digits and periods up to the last period, as in `12.` or `12...e4`.
    let number_end = token
        .char_indices()
        .take_while(|&(_, c)| c.is_ascii_digit() || c == '.')
        .filter(|&(_, c)| c == '.')
        .last()
        .map_or(0, |(dot, _)| dot + 1);
    let san = &token[number_end..];
    let is_comment_tag_or_nag = lexeme.starts_with(['{', ';', '[', '$']);
    let is_result = ["1-0", "0-1", "1/2-1/2", "*"].contains(&san);
    // Also true when nothing is left, as for a glyph on its own.
    let is_number = san.chars().all(|c| c.is_ascii_digit());
    (!is_comment_tag_or_nag && !is_result && !is_number).then_some(san)
}
