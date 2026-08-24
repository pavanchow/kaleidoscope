use std::fmt;

/// Every stage in the pipeline returns this error type instead of panicking
/// on malformed or pathological input. Parsing untrusted documents should
/// never crash the process, it should fail with a reason.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)] // InvalidLength/InvalidColor/Io are part of the typed-error surface for future stages
pub enum EngineError {
    InputTooLarge { limit: usize, actual: usize },
    NestingTooDeep { limit: usize },
    LayoutTooDeep { limit: usize },
    InvalidCss(String),
    InvalidLength(String),
    InvalidColor(String),
    Io(String),
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EngineError::InputTooLarge { limit, actual } => {
                write!(f, "input too large: {actual} bytes exceeds limit of {limit} bytes")
            }
            EngineError::NestingTooDeep { limit } => {
                write!(f, "nesting exceeds max depth of {limit}")
            }
            EngineError::LayoutTooDeep { limit } => {
                write!(f, "layout recursion exceeds max depth of {limit}")
            }
            EngineError::InvalidCss(msg) => write!(f, "invalid css: {msg}"),
            EngineError::InvalidLength(msg) => write!(f, "invalid length: {msg}"),
            EngineError::InvalidColor(msg) => write!(f, "invalid color: {msg}"),
            EngineError::Io(msg) => write!(f, "io error: {msg}"),
        }
    }
}

impl std::error::Error for EngineError {}

pub type EngineResult<T> = Result<T, EngineError>;

/// Resource bounds enforced across every stage. These exist so pathological
/// input (a million nested divs, a gigabyte stylesheet) fails fast with a
/// typed error instead of exhausting the stack or the heap.
pub const MAX_INPUT_BYTES: usize = 5 * 1024 * 1024;
pub const MAX_DOM_DEPTH: usize = 512;
pub const MAX_CSS_DEPTH: usize = 256;
pub const MAX_LAYOUT_DEPTH: usize = 512;

pub fn check_input_size(input: &str) -> EngineResult<()> {
    if input.len() > MAX_INPUT_BYTES {
        return Err(EngineError::InputTooLarge {
            limit: MAX_INPUT_BYTES,
            actual: input.len(),
        });
    }
    Ok(())
}
