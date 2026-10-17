use crate::chess::Color;

/// One of the figures a side moves on the board, e.g. a White knight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Piece {
    pub color: Color,
    pub kind: PieceKind,
}

/// What a [piece](Piece) is, which decides how it moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PieceKind {
    Pawn,
    Knight,
    Bishop,
    Rook,
    Queen,
    King,
}

impl PieceKind {
    /// The kind a [SAN](crate::Ply) letter names. SAN gives pawns no letter.
    pub(crate) fn from_char(c: char) -> Option<PieceKind> {
        match c {
            'N' => Some(PieceKind::Knight),
            'B' => Some(PieceKind::Bishop),
            'R' => Some(PieceKind::Rook),
            'Q' => Some(PieceKind::Queen),
            'K' => Some(PieceKind::King),
            _ => None,
        }
    }
}

impl Piece {
    pub(crate) fn from_char(c: char) -> Option<Piece> {
        let kind = match c.to_ascii_uppercase() {
            'P' => PieceKind::Pawn,
            upper => PieceKind::from_char(upper)?,
        };
        let color = if c.is_ascii_uppercase() {
            Color::White
        } else {
            Color::Black
        };
        Some(Piece { color, kind })
    }

    pub(crate) fn to_char(self) -> char {
        let c = match self.kind {
            PieceKind::Pawn => 'P',
            PieceKind::Knight => 'N',
            PieceKind::Bishop => 'B',
            PieceKind::Rook => 'R',
            PieceKind::Queen => 'Q',
            PieceKind::King => 'K',
        };
        match self.color {
            Color::White => c,
            Color::Black => c.to_ascii_lowercase(),
        }
    }
}
