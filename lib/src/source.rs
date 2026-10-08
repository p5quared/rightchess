use crate::{Error, Game, SourceKind, YearMonth};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceQuery {
    pub username: String,
    /// Only load batches from this month onward.
    pub since: Option<YearMonth>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BatchKey {
    pub source: SourceKind,
    pub id: String,
}

/// A source-defined unit of fetching, e.g. one chess.com monthly archive.
pub trait Batch {
    fn key(&self) -> BatchKey;
    /// True once the batch's games can no longer change.
    fn is_sealed(&self) -> bool;
}

pub trait GameSource {
    type Batch: Batch;

    fn batches(&self, query: &SourceQuery) -> Result<Vec<Self::Batch>, Error>;
    fn fetch(&self, batch: &Self::Batch) -> Result<Vec<Game>, Error>;
}
