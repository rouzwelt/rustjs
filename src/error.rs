use thiserror::Error;
use deno_runtime::deno_core::url::ParseError;
use deno_runtime::deno_core::anyhow::Error as AnyhowError;

#[derive(Error, Debug)]
pub enum Error {
    #[error("failed to parse file path to url: {0}")]
    FailedToParseFilePathToUrl(String),

    #[error(transparent)]
    ParseUrlError(#[from] ParseError),

    #[error(transparent)]
    DenoError(#[from] AnyhowError),

    #[error("duplicate side modules names: {0}")]
    DuplicateSideModules(String),

    #[error(transparent)]
    SerdeV8Error(#[from] deno_runtime::deno_core::serde_v8::Error),
}
