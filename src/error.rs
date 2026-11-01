use crate::client::FetchError;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("manifest is not loaded")]
    ManifestNotLoaded,
    #[error("unsupported catalog schema version `{0}`")]
    UnsupportedSchema(String),
    #[error("invalid base url: {0}")]
    BaseUrl(String),
    #[error("invalid configuration: {0}")]
    Config(String),
    #[error(transparent)]
    Fetch(#[from] FetchError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}
