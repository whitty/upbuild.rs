pub mod error;
pub mod file;
pub mod exec;
pub mod find;

pub type Error = error::Error;
pub type Result<T> = std::result::Result<T, error::Error>;
