use std::fmt;

use anyhow::{Context, Result, anyhow, bail, ensure};

use crate::{Fen, Ply};

/// A position in standard chess: enough to play SAN moves and write FENs.
#[derive(Debug, Clone)]
pub struct Board {
    squares: [Option<Piece>; 64],
    side: Color,
    /// In FEN's order, as in `RIGHTS`.
    castling: [bool; 4],
    en_passant: Option<Square>,
    halfmove: u32,
    fullmove: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Color {
    White,
    Black,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Pawn,
    Knight,
    Bishop,
    Rook,
    Queen,
    King,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Piece {
    color: Color,
    kind: Kind,
}

/// A square, with files and ranks counted from 0: a1 is (0, 0) and h8 is (7, 7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Square {
    file: i8,
    rank: i8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Wing {
    King,
    Queen,
}

/// The castling rights in FEN's order: `KQkq`.
const RIGHTS: [(Color, Wing); 4] = [
    (Color::White, Wing::King),
    (Color::White, Wing::Queen),
    (Color::Black, Wing::King),
    (Color::Black, Wing::Queen),
];

/// A move resolved against a board.
#[derive(Debug, Clone, Copy)]
struct Move {
    from: Square,
    to: Square,
    promotion: Option<Kind>,
    castling: Option<Wing>,
}

impl Board {
    /// The position `fen` describes, if it could be played from: each side has one king, no
    /// pawn is on the first or last rank, the side that just moved isn't in check, castling
    /// rights have their king and rook at home, and the en passant square is behind a pawn that
    /// just moved two squares.
    pub fn from_fen(fen: &Fen) -> Result<Board> {
        let board = Board::parse(fen.as_str()).with_context(|| format!("{fen} isn't a FEN"))?;
        board
            .check()
            .map_err(|reason| anyhow!("{fen} can't be played: {reason}"))?;
        Ok(board)
    }

    pub fn fen(&self) -> Fen {
        let mut text = String::new();
        for rank in (0..8).rev() {
            let mut empty = 0;
            for file in 0..8 {
                match self.at(Square { file, rank }) {
                    None => empty += 1,
                    Some(piece) => {
                        if empty > 0 {
                            text += &empty.to_string();
                            empty = 0;
                        }
                        text.push(piece.to_char());
                    }
                }
            }
            if empty > 0 {
                text += &empty.to_string();
            }
            if rank > 0 {
                text.push('/');
            }
        }
        let side = match self.side {
            Color::White => 'w',
            Color::Black => 'b',
        };
        let castling: String = "KQkq"
            .chars()
            .zip(self.castling)
            .filter_map(|(c, right)| right.then_some(c))
            .collect();
        let castling = if castling.is_empty() {
            "-".to_string()
        } else {
            castling
        };
        let en_passant = self
            .en_passant
            .map_or("-".to_string(), |square| square.to_string());
        Fen(format!(
            "{text} {side} {castling} {en_passant} {} {}",
            self.halfmove, self.fullmove
        ))
    }

    /// The position after `ply`, or an error naming it if it can't be played here.
    pub fn play(&self, ply: &Ply) -> Result<Board> {
        let san = ply.as_str().trim_end_matches(['+', '#']);
        if san == "--" {
            return Ok(self.pass());
        }
        let number = match self.side {
            Color::White => format!("{}.", self.fullmove),
            Color::Black => format!("{}...", self.fullmove),
        };
        let mv = self.resolve(san).map_err(|reason| {
            anyhow!("{number} {ply} can't be played in {}: {reason}", self.fen())
        })?;
        Ok(self.apply(mv))
    }

    fn parse(fen: &str) -> Result<Board> {
        let fields: Vec<&str> = fen.split(' ').collect();
        let [board, side, castling, en_passant, halfmove, fullmove] = fields[..] else {
            bail!("it doesn't have 6 fields");
        };
        let mut squares = [None; 64];
        for (row, text) in board.split('/').enumerate() {
            let rank = 7 - row as i8;
            let mut file = 0;
            for c in text.chars() {
                match c.to_digit(10) {
                    Some(empty) => file += empty as i8,
                    None => {
                        let square = Square::new(file, rank).context("a rank is too long")?;
                        squares[square.index()] = Some(Piece::from_char(c).context("a piece")?);
                        file += 1;
                    }
                }
            }
        }
        Ok(Board {
            squares,
            side: if side == "w" {
                Color::White
            } else {
                Color::Black
            },
            castling: ['K', 'Q', 'k', 'q'].map(|c| castling.contains(c)),
            en_passant: Square::parse(en_passant),
            halfmove: halfmove.parse()?,
            fullmove: fullmove.parse()?,
        })
    }

    /// Fails with the reason this position can't be played from.
    fn check(&self) -> Result<()> {
        for color in [Color::White, Color::Black] {
            let king = Piece {
                color,
                kind: Kind::King,
            };
            let kings = self
                .squares
                .iter()
                .filter(|&&piece| piece == Some(king))
                .count();
            ensure!(kings == 1, "{color:?} has {kings} kings");
        }
        let pawn_on_edge = Square::all().any(|square| {
            (square.rank == 0 || square.rank == 7)
                && self
                    .at(square)
                    .is_some_and(|piece| piece.kind == Kind::Pawn)
        });
        ensure!(!pawn_on_edge, "a pawn is on the first or last rank");
        ensure!(
            !self.in_check(self.side.other()),
            "{:?} isn't to move but is in check",
            self.side.other()
        );
        for (&(color, wing), _) in RIGHTS.iter().zip(self.castling).filter(|(_, right)| *right) {
            let at_home = self.at(king_home(color))
                == Some(Piece {
                    color,
                    kind: Kind::King,
                })
                && self.at(rook_home(color, wing))
                    == Some(Piece {
                        color,
                        kind: Kind::Rook,
                    });
            ensure!(
                at_home,
                "{color:?} may castle {wing:?}side without king and rook at home"
            );
        }
        if let Some(square) = self.en_passant {
            let mover = self.side.other();
            let pawn = Piece {
                color: mover,
                kind: Kind::Pawn,
            };
            let behind = square.offset(0, -mover.forward());
            let ahead = square.offset(0, mover.forward());
            let passed = self.at(square).is_none()
                && behind.is_some_and(|behind| self.at(behind).is_none())
                && ahead.is_some_and(|ahead| self.at(ahead) == Some(pawn));
            ensure!(passed, "no pawn just passed the en passant square {square}");
        }
        Ok(())
    }

    /// The move `san` names, with any `+` or `#` removed. Fails with the reason it can't be
    /// played.
    fn resolve(&self, san: &str) -> Result<Move> {
        match san {
            "O-O" | "0-0" => return self.castle(Wing::King),
            "O-O-O" | "0-0-0" => return self.castle(Wing::Queen),
            _ if san.contains('@') => bail!("drops aren't standard chess"),
            _ => {}
        }
        let (kind, rest) = match Kind::from_char(san.chars().next().context("no move")?) {
            Some(kind) => (kind, &san[1..]),
            None => (Kind::Pawn, san),
        };
        let (rest, promotion) = match rest.split_once('=') {
            Some((rest, piece)) => (rest, piece.chars().next().and_then(Kind::from_char)),
            None => (rest, None),
        };
        ensure!(promotion != Some(Kind::King), "a pawn can't become a king");
        let (from, to) = rest.split_at(rest.len() - 2);
        let to = Square::parse(to).context("no target square")?;
        let (from, capture) = match from.strip_suffix('x') {
            Some(from) => (from, true),
            None => (from, false),
        };
        match self.at(to) {
            Some(piece) if piece.color == self.side => bail!("a piece of its own is on {to}"),
            Some(_) => ensure!(capture, "it captures on {to} without an x"),
            None => ensure!(
                !capture || (kind == Kind::Pawn && self.en_passant == Some(to)),
                "nothing is on {to} to capture"
            ),
        }
        let piece = Piece {
            color: self.side,
            kind,
        };
        let moves: Vec<Move> = Square::all()
            .filter(|&square| self.at(square) == Some(piece) && square.matches(from))
            .filter(|&square| self.reaches(square, to, capture))
            .map(|from| Move {
                from,
                to,
                promotion,
                castling: None,
            })
            .collect();
        ensure!(!moves.is_empty(), "no {kind:?} can move to {to}");
        let legal: Vec<Move> = moves.into_iter().filter(|&mv| self.is_legal(mv)).collect();
        match legal[..] {
            [mv] => Ok(mv),
            [] => bail!("it leaves the king in check"),
            _ => bail!("{} pieces can make it", legal.len()),
        }
    }

    fn castle(&self, wing: Wing) -> Result<Move> {
        let color = self.side;
        let right = RIGHTS.iter().position(|&right| right == (color, wing));
        ensure!(
            right.is_some_and(|right| self.castling[right]),
            "{color:?} may not castle {wing:?}side"
        );
        let rank = king_home(color).rank;
        let square = |file| Square { file, rank };
        let (between, path): (&[i8], [i8; 2]) = match wing {
            Wing::King => (&[5, 6], [5, 6]),
            Wing::Queen => (&[1, 2, 3], [3, 2]),
        };
        ensure!(
            between.iter().all(|&file| self.at(square(file)).is_none()),
            "pieces are in the way"
        );
        ensure!(!self.in_check(color), "the king is in check");
        ensure!(
            path.iter()
                .all(|&file| !self.is_attacked(square(file), color.other())),
            "the king passes through or lands on an attacked square"
        );
        Ok(Move {
            from: king_home(color),
            to: square(path[1]),
            promotion: None,
            castling: Some(wing),
        })
    }

    /// Whether the piece on `from` can move to `to`, ignoring checks. A pawn moves straight
    /// unless it captures.
    fn reaches(&self, from: Square, to: Square, capture: bool) -> bool {
        let Some(piece) = self.at(from) else {
            return false;
        };
        if piece.kind != Kind::Pawn || capture {
            return self.attacks(from, to);
        }
        let forward = piece.color.forward();
        let (files, ranks) = (to.file - from.file, to.rank - from.rank);
        let start = if piece.color == Color::White { 1 } else { 6 };
        let empty = |square: Option<Square>| square.is_some_and(|square| self.at(square).is_none());
        files == 0
            && empty(Some(to))
            && (ranks == forward
                || (ranks == 2 * forward && from.rank == start && empty(from.offset(0, forward))))
    }

    /// Whether the piece on `from` attacks `to`.
    fn attacks(&self, from: Square, to: Square) -> bool {
        let Some(piece) = self.at(from) else {
            return false;
        };
        let (files, ranks) = (to.file - from.file, to.rank - from.rank);
        let diagonal = files != 0 && files.abs() == ranks.abs();
        let straight = (files == 0) != (ranks == 0);
        match piece.kind {
            Kind::Pawn => ranks == piece.color.forward() && files.abs() == 1,
            Kind::Knight => matches!((files.abs(), ranks.abs()), (1, 2) | (2, 1)),
            Kind::King => files.abs().max(ranks.abs()) == 1,
            Kind::Bishop => diagonal && self.is_clear(from, to),
            Kind::Rook => straight && self.is_clear(from, to),
            Kind::Queen => (diagonal || straight) && self.is_clear(from, to),
        }
    }

    /// Whether the squares strictly between `from` and `to`, on a line, are empty.
    fn is_clear(&self, from: Square, to: Square) -> bool {
        let (files, ranks) = (
            (to.file - from.file).signum(),
            (to.rank - from.rank).signum(),
        );
        let mut square = from;
        loop {
            square = Square {
                file: square.file + files,
                rank: square.rank + ranks,
            };
            if square == to {
                return true;
            }
            if self.at(square).is_some() {
                return false;
            }
        }
    }

    fn is_attacked(&self, square: Square, by: Color) -> bool {
        Square::all().any(|from| {
            self.at(from).is_some_and(|piece| piece.color == by) && self.attacks(from, square)
        })
    }

    fn in_check(&self, color: Color) -> bool {
        let king = Some(Piece {
            color,
            kind: Kind::King,
        });
        Square::all()
            .find(|&square| self.at(square) == king)
            .is_some_and(|square| self.is_attacked(square, color.other()))
    }

    fn is_legal(&self, mv: Move) -> bool {
        !self.apply(mv).in_check(self.side)
    }

    fn apply(&self, mv: Move) -> Board {
        let mut next = self.clone();
        let Some(piece) = self.at(mv.from) else {
            return next;
        };
        let en_passant_capture =
            piece.kind == Kind::Pawn && mv.from.file != mv.to.file && self.at(mv.to).is_none();
        let captures = self.at(mv.to).is_some() || en_passant_capture;
        if en_passant_capture {
            next.squares[Square {
                file: mv.to.file,
                rank: mv.from.rank,
            }
            .index()] = None;
        }
        next.squares[mv.from.index()] = None;
        next.squares[mv.to.index()] = Some(Piece {
            kind: mv.promotion.unwrap_or(piece.kind),
            ..piece
        });
        if let Some(wing) = mv.castling {
            let rook = rook_home(piece.color, wing);
            let file = if wing == Wing::King { 5 } else { 3 };
            next.squares[rook.index()] = None;
            next.squares[Square {
                file,
                rank: rook.rank,
            }
            .index()] = Some(Piece {
                kind: Kind::Rook,
                ..piece
            });
        }
        for (right, &(color, wing)) in next.castling.iter_mut().zip(&RIGHTS) {
            let king_moved = piece.kind == Kind::King && piece.color == color;
            let rook_gone = [mv.from, mv.to].contains(&rook_home(color, wing));
            *right &= !king_moved && !rook_gone;
        }
        let double_push = piece.kind == Kind::Pawn && (mv.to.rank - mv.from.rank).abs() == 2;
        next.en_passant = double_push.then(|| Square {
            file: mv.from.file,
            rank: (mv.from.rank + mv.to.rank) / 2,
        });
        next.halfmove = if piece.kind == Kind::Pawn || captures {
            0
        } else {
            self.halfmove.saturating_add(1)
        };
        next.end_turn();
        next
    }

    /// The position after a null move: the other side is to move.
    fn pass(&self) -> Board {
        let mut next = self.clone();
        next.en_passant = None;
        next.halfmove = self.halfmove.saturating_add(1);
        next.end_turn();
        next
    }

    fn end_turn(&mut self) {
        if self.side == Color::Black {
            self.fullmove = self.fullmove.saturating_add(1);
        }
        self.side = self.side.other();
    }

    fn at(&self, square: Square) -> Option<Piece> {
        self.squares[square.index()]
    }
}

impl Color {
    fn other(self) -> Color {
        match self {
            Color::White => Color::Black,
            Color::Black => Color::White,
        }
    }

    /// The way this side's pawns move along the files: up for White.
    fn forward(self) -> i8 {
        match self {
            Color::White => 1,
            Color::Black => -1,
        }
    }
}

impl Kind {
    /// The piece a SAN letter names. Pawns have none.
    fn from_char(c: char) -> Option<Kind> {
        match c {
            'N' => Some(Kind::Knight),
            'B' => Some(Kind::Bishop),
            'R' => Some(Kind::Rook),
            'Q' => Some(Kind::Queen),
            'K' => Some(Kind::King),
            _ => None,
        }
    }
}

impl Piece {
    /// The piece a FEN letter names: uppercase for White.
    fn from_char(c: char) -> Option<Piece> {
        let kind = match c.to_ascii_uppercase() {
            'P' => Kind::Pawn,
            upper => Kind::from_char(upper)?,
        };
        let color = if c.is_ascii_uppercase() {
            Color::White
        } else {
            Color::Black
        };
        Some(Piece { color, kind })
    }

    fn to_char(self) -> char {
        let c = match self.kind {
            Kind::Pawn => 'P',
            Kind::Knight => 'N',
            Kind::Bishop => 'B',
            Kind::Rook => 'R',
            Kind::Queen => 'Q',
            Kind::King => 'K',
        };
        match self.color {
            Color::White => c,
            Color::Black => c.to_ascii_lowercase(),
        }
    }
}

impl Square {
    fn new(file: i8, rank: i8) -> Option<Square> {
        ((0..8).contains(&file) && (0..8).contains(&rank)).then_some(Square { file, rank })
    }

    /// The square written like `e4`.
    fn parse(text: &str) -> Option<Square> {
        match *text.as_bytes() {
            [file @ b'a'..=b'h', rank @ b'1'..=b'8'] => {
                Square::new((file - b'a') as i8, (rank - b'1') as i8)
            }
            _ => None,
        }
    }

    fn all() -> impl Iterator<Item = Square> {
        (0..64).map(|i| Square {
            file: i % 8,
            rank: i / 8,
        })
    }

    fn index(self) -> usize {
        (self.rank * 8 + self.file) as usize
    }

    fn offset(self, files: i8, ranks: i8) -> Option<Square> {
        Square::new(self.file + files, self.rank + ranks)
    }

    /// Whether this square fits a SAN disambiguation: nothing, a file, a rank or a square.
    fn matches(self, hint: &str) -> bool {
        hint.bytes().all(|c| match c {
            b'a'..=b'h' => (c - b'a') as i8 == self.file,
            b'1'..=b'8' => (c - b'1') as i8 == self.rank,
            _ => false,
        })
    }
}

impl fmt::Display for Square {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", (b'a' + self.file as u8) as char, self.rank + 1)
    }
}

fn king_home(color: Color) -> Square {
    let rank = if color == Color::White { 0 } else { 7 };
    Square { file: 4, rank }
}

fn rook_home(color: Color, wing: Wing) -> Square {
    let file = if wing == Wing::King { 7 } else { 0 };
    Square {
        file,
        ..king_home(color)
    }
}
