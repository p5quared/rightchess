/// A calendar day, e.g. `2024.03.15` in [PGN](crate::Pgn). Its fields aren't checked, so it can
/// name a day that doesn't exist. Fields are declared largest first, so the derived order is
/// chronological.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date {
    pub year: u16,
    pub month: u8,
    pub day: u8,
}
