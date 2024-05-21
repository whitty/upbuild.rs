use super::exec::RetCode;

#[derive(Debug)]
pub enum Error {
    InvalidTag(String),
    InvalidRetMapDefinition(String),
    EmptyEntry,
    FlagBeforeCommand(String),
    NoCommands,
    FailedToExec(std::io::Error),
    IoFailed(std::io::Error),
    InvalidDir(String),
    NotFound(String),
    ExitWithExitCode(RetCode),
    ExitWithSignal(RetCode),
    UnableToReadOutfile(String, std::io::Error),
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
            Error::IoFailed(e) =>
                write!(f, "{}", e),
            Error::InvalidDir(p) =>
                write!(f, "Invalid directory '{}'", p),
            Error::NotFound(p) =>
                write!(f, "Unable to locate .upbuild from '{}'", p),
            Error::ExitWithExitCode(c) =>
                 write!(f, "Process exitted with code: {}", c),
            Error::ExitWithSignal(c) =>
                 write!(f, "Process exitted with signal: {}", c),
            Error::UnableToReadOutfile(file, e) =>
                write!(f, "Unable to read @outfile={}: {}", file, e),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match *self {
            Error::InvalidTag(_) | Error::InvalidRetMapDefinition(_) |
            Error::EmptyEntry | Error::FlagBeforeCommand(_) |
            Error::NoCommands | Error::ExitWithExitCode(_) |
            Error::ExitWithSignal(_) | Error::InvalidDir(_) | Error::NotFound(_) |
            Error::UnableToReadOutfile(_, _)

                => None,

            Error::FailedToExec(ref e) => Some(e),
            Error::IoFailed(ref e) => Some(e),
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Error {
        Error::IoFailed(err)
    }
}
