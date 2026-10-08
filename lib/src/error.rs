#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("http request failed: {0}")]
    Http(#[from] ureq::Error),
    #[error("unexpected response: {0}")]
    UnexpectedResponse(String),
}
