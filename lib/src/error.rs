#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("http request failed: {0}")]
    Http(#[from] ureq::Error),
    #[error("unexpected archive url: {0}")]
    BadArchiveUrl(String),
}
