mod error;
mod file;
mod exec;
mod find;
mod args;

pub use file::ClassicFile;

pub use exec::Exec;
pub use exec::process_runner;
pub use exec::print_runner;

pub use find::find;
pub use args::Config;

pub type Error = error::Error;
pub type Result<T> = std::result::Result<T, error::Error>;
