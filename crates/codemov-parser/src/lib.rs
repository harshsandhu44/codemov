pub mod rust;
mod tests;
pub mod typescript;

use codemov_core::{Language, Symbol};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("unsupported language: {0:?}")]
    UnsupportedLanguage(Language),
    #[error("parse error: {0}")]
    Parse(String),
    #[error("utf8 error: {0}")]
    Utf8(#[from] std::str::Utf8Error),
}

pub fn extract_symbols(source: &[u8], language: Language) -> Result<Vec<Symbol>, ParseError> {
    match language {
        Language::Rust => rust::extract(source),
        Language::TypeScript | Language::JavaScript => typescript::extract(source, language),
        Language::Unknown => Ok(vec![]),
    }
}
