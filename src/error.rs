use super::exec::RetCode;

#[derive(Debug)]
pub enum Error {
    InvalidTag(String),
    InvalidRetMapDefinition(String),
    EmptyEntry,
    FlagBeforeCommand(String),
    NoCommands,
    FailedToExec(std::io::Error),
    ExitWithExitCode(RetCode),
    ExitWithSignal(RetCode),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match &self {
            Error::InvalidTag(s) =>
                write!(f, "Tag was not understood: {}", s),
            Error::InvalidRetMapDefinition(s) =>
                write!(f, "Unable to parse retmap from: {}", s),
            Error::EmptyEntry =>
                write!(f, "Empty entry"),
            Error::FlagBeforeCommand(s) =>
                write!(f, "Found tag before command {}", s),
            Error::NoCommands =>
                write!(f, "No commands in file"),
            Error::FailedToExec(e) =>
                 write!(f, "Failed to exec: {}", e),
            Error::ExitWithExitCode(c) =>
                 write!(f, "Process exitted with code: {}", c),
            Error::ExitWithSignal(c) =>
                 write!(f, "Process exitted with signal: {}", c),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match *self {
            Error::InvalidTag(_) | Error::InvalidRetMapDefinition(_) |
            Error::EmptyEntry | Error::FlagBeforeCommand(_) |
            Error::NoCommands | Error::ExitWithExitCode(_) |
            Error::ExitWithSignal(_)
                => None,

            Error::FailedToExec(ref e) => Some(e),
        }
    }
}
