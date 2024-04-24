use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("failed to parse file path to url: {0}")]
    FailedToParseFilePathToUrl(String),
}
