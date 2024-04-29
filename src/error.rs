use thiserror::Error;
use deno_runtime::deno_core::{v8, serde_v8};
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
    SerdeV8Error(#[from] serde_v8::Error),

    #[error("failed to get js value")]
    FailedToGetV8Value,

    #[error("undefined export")]
    UndefinedExport,

    #[error(transparent)]
    V8DataError(#[from] v8::DataError),

    #[error("an unexpected error occured")]
    UnexpectedError,

    #[error("js exception: {}", 0.0)]
    JsException((String, Option<serde_json::Value>)),
}

pub fn catch_exception(
    try_catch_scope: &mut v8::TryCatch<'_, v8::HandleScope<'_, v8::Context>>,
) -> Error {
    if try_catch_scope.has_caught() {
        let msg = try_catch_scope
            .stack_trace()
            .or_else(|| try_catch_scope.exception())
            .map(|value| value.to_rust_string_lossy(try_catch_scope))
            .unwrap_or_else(|| "no exception".into());

        return Error::JsException((msg, None));
    }
    Error::UnexpectedError
}
