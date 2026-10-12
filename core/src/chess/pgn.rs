use std::ops::ControlFlow;
use std::str::FromStr;
use std::{fmt, iter};

use anyhow::{Context, Result, anyhow, bail, ensure};

use crate::chess::board::Board;
use crate::chess::{Fen, Ply};

/// One game in PGN, as written. Only well-formed text is a `Pgn`: comments, variations and tags
/// are closed, tags are `[Name "value"]` pairs before the movetext, and only comments follow
/// the result. Moves aren't checked; `plies` does that.
///
/// PGN (Portable Game Notation) is the standard text format for a game: tags, which are
/// metadata like `[White "Carlsen"]`, then the movetext, the moves with their numbers as in
/// `1. e4 e5 2. Nf3`, ending in a result: `1-0` (White won), `0-1`, `1/2-1/2` (a draw) or `*`
/// (unfinished). Comments go in `{}` and variations, alternative lines that weren't played, in
/// `()`; the mainline is the moves actually played. A move may carry a glyph, an annotation like
/// `!` or `?!`, or a NAG (Numeric Annotation Glyph), the same as a number such as `$14`. A
/// `[FEN]` tag starts the game from a custom position, and a `[Variant]` tag names a variant, a
/// game with other rules such as Chess960 or Crazyhouse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pgn(String);

impl Pgn {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The [mainline](Pgn)'s [plies](Ply), in order. Each is a move as written minus trailing
    /// [glyphs](Pgn); [tags](Pgn), comments, [variations](Pgn), [NAGs](Pgn), move numbers and the
    /// result are skipped. Moves aren't checked for legality.
    ///
    /// # Errors
    ///
    /// Fails only on a move that isn't [SAN](Ply).
    pub fn plies(&self) -> Result<Vec<Ply>> {
        mainline(&self.0)?
            .words
            .into_iter()
            .filter_map(move_text)
            .map(str::parse)
            .collect()
    }

    /// The unescaped value of the first [tag](Pgn) named `name`, e.g. `Some("180+2")` for
    /// `"TimeControl"`. Names are case-sensitive, and tags in comments or [variations](Pgn) don't
    /// count.
    #[must_use]
    pub fn tag(&self, name: &str) -> Option<String> {
        mainline(&self.0)
            .ok()?
            .tags
            .into_iter()
            .find_map(|(tag, value)| (tag == name).then_some(value))
    }

    /// The [mainline](Pgn)'s positions, as [FENs](Fen): the start, then one after each
    /// [ply](Ply). The start is the [`[FEN]` tag](Pgn), or the standard starting position
    /// without one. Moves are played by the rules of standard chess; other [variants](Pgn) are an
    /// error. Yields an error naming the first ply that can't be played, then stops.
    pub fn fens(&self) -> impl Iterator<Item = Result<Fen>> + '_ {
        let (start, words) = match mainline(&self.0) {
            Ok(mainline) => (start(&mainline.tags), mainline.words),
            Err(err) => (Err(err), Vec::new()),
        };
        let mut moves = words.into_iter().filter_map(move_text);
        let mut next = Some(start);
        iter::from_fn(move || {
            let board = next.take()?;
            Some(board.map(|board| {
                next = moves
                    .next()
                    .map(|san| san.parse().and_then(|ply| board.play(&ply)));
                board.fen()
            }))
        })
    }
}

impl FromStr for Pgn {
    type Err = anyhow::Error;

    fn from_str(text: &str) -> Result<Self> {
        mainline(text)?;
        Ok(Pgn(text.to_string()))
    }
}

impl fmt::Display for Pgn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

const RESULTS: [&str; 4] = ["1-0", "0-1", "1/2-1/2", "*"];

/// The part of a game the [mainline](Pgn) has reached.
enum Section {
    Tags,
    Movetext,
    AfterResult,
}

/// The position a game starts from: its [`[FEN]` tag](Pgn), or the standard starting position.
/// Fails for [variants](Pgn) other than standard chess.
fn start(tags: &[(&str, String)]) -> Result<Board> {
    let tag = |name| {
        tags.iter()
            .find(|(tag, _)| *tag == name)
            .map(|(_, value)| value.as_str())
    };
    if let Some(variant) = tag("Variant") {
        ensure!(
            matches!(variant, "Standard" | "From Position"),
            "unsupported variant {variant}"
        );
    }
    let fen = match tag("FEN") {
        Some(fen) => fen.parse()?,
        None => Fen::starting(),
    };
    Board::from_fen(&fen)
}

/// The [tags](Pgn) and words of a game outside comments and [variations](Pgn).
struct Mainline<'a> {
    /// Names and values, in order.
    tags: Vec<(&'a str, String)>,
    /// Moves, move numbers, [glyphs](Pgn), [NAGs](Pgn) and the [result](Pgn).
    words: Vec<&'a str>,
}

/// The [tags](Pgn) and words outside comments and [variations](Pgn). Fails unless `text` is one
/// game: delimiters close, tags are well-formed pairs before the [movetext](Pgn), and only
/// comments follow the [result](Pgn).
fn mainline(text: &str) -> Result<Mainline<'_>> {
    let mut tags = Vec::new();
    let mut words = Vec::new();
    let mut section = Section::Tags;
    let mut depth = 0u32;
    for lexeme in lexemes(text) {
        let lexeme = lexeme?;
        if depth > 0 {
            match lexeme {
                "(" => depth += 1,
                ")" => depth -= 1,
                _ => {}
            }
            continue;
        }
        if lexeme.starts_with(['{', ';']) {
            continue;
        }
        ensure!(
            !matches!(section, Section::AfterResult),
            "{lexeme} after the result"
        );
        if lexeme.starts_with('[') {
            let pair = tag_pair(lexeme).with_context(|| format!("malformed tag {lexeme}"))?;
            ensure!(
                matches!(section, Section::Tags),
                "{lexeme} after the movetext"
            );
            tags.push(pair);
            continue;
        }
        match lexeme {
            ")" => bail!("unmatched )"),
            "(" => depth = 1,
            word => words.push(word),
        }
        section = if RESULTS.contains(&lexeme) {
            Section::AfterResult
        } else {
            Section::Movetext
        };
    }
    ensure!(depth == 0, "unterminated variation");
    Ok(Mainline { tags, words })
}

/// The name and unescaped value of a [tag](Pgn) pair like `[Event "x"]`: a name of letters, digits
/// and `_` that doesn't start with `_`, then one quoted value, with optional whitespace around
/// each.
fn tag_pair(tag: &str) -> Option<(&str, String)> {
    let inner = tag.strip_prefix('[')?.strip_suffix(']')?;
    let inner = inner.trim_start();
    let name_len = inner
        .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .unwrap_or(inner.len());
    let (name, rest) = inner.split_at(name_len);
    let value = rest.trim_start().strip_prefix('"')?;
    let (value, after_value) = value.split_at(quoted_len(value)?);
    let well_formed = !name.is_empty() && !name.starts_with('_') && after_value.trim().is_empty();
    well_formed.then(|| (name, unescape(&value[..value.len() - 1])))
}

/// A quoted value without its `\` escapes.
fn unescape(value: &str) -> String {
    let mut escaped = false;
    value
        .chars()
        .filter(|&c| {
            let keep = escaped || c != '\\';
            escaped = !escaped && c == '\\';
            keep
        })
        .collect()
}

/// The length of a quoted value up to and including its closing `"`, where `\"` and `\\` are
/// escapes. `value` starts after the opening `"`.
fn quoted_len(value: &str) -> Option<usize> {
    let mut escaped = false;
    let (close, _) = value.char_indices().find(|&(_, c)| {
        let closes = !escaped && c == '"';
        escaped = !escaped && c == '\\';
        closes
    })?;
    Some(close + 1)
}

/// The comments, [tags](Pgn), [NAGs](Pgn), parentheses and words of `text`, ending at the first
/// error.
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

/// The length of the [tag](Pgn) pair `text` starts with, such as `[Event "Live Chess"]`. Its quoted
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

/// The move in a word without its move number or [glyphs](Pgn), or None if the word holds no
/// move: a [NAG](Pgn), move number, glyph or [result](Pgn).
fn move_text(word: &str) -> Option<&str> {
    let token = word.trim_end_matches(['!', '?']);
    // A move number is digits and periods up to the last period, as in `12.` or `12...e4`.
    let number_end = token
        .char_indices()
        .take_while(|&(_, c)| c.is_ascii_digit() || c == '.')
        .filter(|&(_, c)| c == '.')
        .last()
        .map_or(0, |(dot, _)| dot + 1);
    let san = &token[number_end..];
    let is_nag = word.starts_with('$');
    let is_result = RESULTS.contains(&san);
    // Also true when nothing is left, as for a glyph on its own.
    let is_number = san.chars().all(|c| c.is_ascii_digit());
    (!is_nag && !is_result && !is_number).then_some(san)
}

#[cfg(test)]
mod tests;
