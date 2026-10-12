//! Generated games for property tests: [PGN](crate::Pgn) text whose moves are known, kept in pieces
//! so that tests can break it in known ways.

#![allow(dead_code, reason = "each test crate uses a different part")]

use proptest::prelude::*;
use proptest::strategy::Union;

/// A move as [SAN](crate::Ply) writes it, e.g. `Nbxd7+`, `e8=Q#` or `O-O-O`.
const SAN: &str = "([KQRBN][a-h]?[1-8]?x?[a-h][1-8]\
    |([a-h]|axb|bxc|cxd|dxe|exf|fxg|gxh|hxg|gxf|fxe|exd|dxc|cxb|bxa)([2-7]|[18]=[QRBN])\
    |O-O(-O)?)[+#]?";
const GLYPH: &str = r"!|\?|!!|\?\?|!\?|\?!";
const MOVE_NUMBER: &str = r"[1-9][0-9]{0,2}\.{0,3}";
/// A move number written against its move, as in `12.e4`.
const NUMBER_PREFIX: &str = r"[1-9][0-9]{0,2}\.{1,3}";
const NAG: &str = r"\$[0-9]{1,3}";
const RESULT: &str = r"1-0|0-1|1/2-1/2|\*";
/// Comment text may hold anything but `}`. These are the characters that could confuse a
/// tokenizer.
const COMMENT: &str = r#"\{[ a-h1-8KQRBNOx=+#.\-(){\[\];$!?"\\→\t\r\n]{0,20}\}"#;
const LINE_COMMENT: &str = r#";[ a-h1-8KQRBNOx=+#.\-(){}\[\];$!?"\\→\t]{0,20}\n"#;
const TAG_NAME: &str = "[A-Z][A-Za-z]{0,8}";
const TAG_VALUE: &str = r#"[ a-h1-8KQRBNOx=+#.\-(){}\[\];$!?"\\→]{0,20}"#;
/// Whitespace that may come before any piece of the [PGN](crate::Pgn).
const SPACE: &str = "[ \t\r\n]{0,2}";
/// Characters that mean something in [PGN](crate::Pgn), so that delimiters meet often.
pub const SOUP: &str = r#"[\[\]{}()$;.!?*0-9a-hKQRBNOx=+#/@\-"\\→ \t\r\n]{0,60}"#;

/// A piece of [PGN](crate::Pgn) text. Words (moves, move numbers, [glyphs](crate::Pgn),
/// [NAGs](crate::Pgn) and [results](crate::Pgn)) need whitespace between them. Comments,
/// [variations](crate::Pgn) and [tags](crate::Pgn) may touch anything.
#[derive(Debug, Clone)]
pub struct Piece {
    space: String,
    text: String,
    word: bool,
}

impl Piece {
    pub fn is_word(&self) -> bool {
        self.word
    }
}

pub fn word(text: impl Into<String>) -> Piece {
    Piece {
        space: String::new(),
        text: text.into(),
        word: true,
    }
}

pub fn group(text: impl Into<String>) -> Piece {
    Piece {
        space: String::new(),
        text: text.into(),
        word: false,
    }
}

fn spaced(piece: impl Strategy<Value = Piece>) -> impl Strategy<Value = Piece> {
    (SPACE, piece).prop_map(|(space, piece)| Piece { space, ..piece })
}

pub fn render(pieces: &[Piece]) -> String {
    let mut text = String::new();
    let mut after_word = false;
    for piece in pieces {
        let needs_space = after_word && piece.word && piece.space.is_empty();
        text += if needs_space {
            " "
        } else {
            piece.space.as_str()
        };
        text += &piece.text;
        after_word = piece.word;
    }
    text
}

/// A generated game: the [plies](crate::Ply) it must give, and the pieces of a [PGN](crate::Pgn)
/// that spells them out.
#[derive(Debug, Clone)]
pub struct Game {
    pub moves: Vec<String>,
    /// The [tags](crate::Pgn)' names and unescaped values, in order.
    pub tag_values: Vec<(String, String)>,
    pub tags: Vec<Piece>,
    /// The [movetext](crate::Pgn) before the [result](crate::Pgn).
    pub body: Vec<Piece>,
    pub result: Option<Piece>,
    /// Comments after the [result](crate::Pgn).
    pub tail: Vec<Piece>,
}

impl Game {
    pub fn pieces(&self) -> Vec<Piece> {
        let mut pieces = self.tags.clone();
        pieces.extend(self.body.iter().cloned());
        pieces.extend(self.result.iter().cloned());
        pieces.extend(self.tail.iter().cloned());
        pieces
    }

    pub fn text(&self) -> String {
        render(&self.pieces())
    }
}

/// The [ply](crate::Ply) a move must give, and the move as written: maybe with its number in front
/// (`12.e4`) and a [glyph](crate::Pgn) behind (`e4!?`).
fn mv() -> impl Strategy<Value = (String, Piece)> {
    (
        SPACE,
        prop::option::of(NUMBER_PREFIX),
        SAN,
        prop::option::of(GLYPH),
    )
        .prop_map(|(space, number, san, glyph)| {
            let number = number.unwrap_or_default();
            let glyph = glyph.unwrap_or_default();
            let text = format!("{number}{san}{glyph}");
            let piece = Piece {
                space,
                text,
                word: true,
            };
            (san, piece)
        })
}

fn comment() -> impl Strategy<Value = Piece> {
    spaced(prop_oneof![
        COMMENT.prop_map(group),
        LINE_COMMENT.prop_map(group)
    ])
}

/// Text that adds no [plies](crate::Ply). [Variations](crate::Pgn) nest up to `depth` deep.
fn filler(depth: u32) -> BoxedStrategy<Piece> {
    let mut kinds = vec![
        spaced(MOVE_NUMBER.prop_map(word)).boxed(),
        spaced(GLYPH.prop_map(word)).boxed(),
        spaced(NAG.prop_map(word)).boxed(),
        comment().boxed(),
    ];
    if depth > 0 {
        let variation =
            movetext(depth - 1).prop_map(|(_, pieces)| group(format!("({})", render(&pieces))));
        kinds.push(spaced(variation).boxed());
    }
    Union::new(kinds).boxed()
}

/// Moves, and pieces that spell them out with filler around each move.
fn movetext(depth: u32) -> BoxedStrategy<(Vec<String>, Vec<Piece>)> {
    let max_plies = 4 + 4 * depth as usize;
    let ply = (prop::collection::vec(filler(depth), 0..3), mv());
    (
        prop::collection::vec(ply, 0..max_plies),
        prop::collection::vec(filler(depth), 0..3),
    )
        .prop_map(|(plies, tail)| {
            let mut moves = Vec::new();
            let mut pieces = Vec::new();
            for (fillers, (san, piece)) in plies {
                pieces.extend(fillers);
                pieces.push(piece);
                moves.push(san);
            }
            pieces.extend(tail);
            (moves, pieces)
        })
        .boxed()
}

/// A game with [tags](crate::Pgn), filler around the moves, maybe a result, and maybe comments
/// after it.
pub fn game() -> impl Strategy<Value = Game> {
    let tag = (TAG_NAME, TAG_VALUE, SPACE).prop_map(|(name, value, space)| {
        let escaped = value.replace('\\', r"\\").replace('"', r#"\""#);
        let piece = Piece {
            space,
            ..group(format!("[{name} \"{escaped}\"]"))
        };
        ((name, value), piece)
    });
    (
        prop::collection::vec(tag, 0..4),
        movetext(2),
        prop::option::of(spaced(RESULT.prop_map(word))),
        prop::collection::vec(comment(), 0..2),
    )
        .prop_map(|(tags, (moves, body), result, tail)| {
            let (tag_values, tags) = tags.into_iter().unzip();
            Game {
                moves,
                tag_values,
                tags,
                body,
                result,
                tail,
            }
        })
}
