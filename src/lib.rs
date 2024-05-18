pub mod error;
pub mod file;

pub type Error = error::Error;
pub type Result<T> = std::result::Result<T, error::Error>;
