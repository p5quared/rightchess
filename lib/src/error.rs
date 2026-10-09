#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("fetching from source failed: {0}")]
    Source(Box<dyn std::error::Error + Send + Sync>),
    #[error("unexpected response: {0}")]
    UnexpectedResponse(String),
    #[error("storage failed: {0}")]
    Storage(Box<dyn std::error::Error + Send + Sync>),
}
