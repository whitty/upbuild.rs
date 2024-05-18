#[derive(Debug)]
pub enum Error {
    InvalidTag(String),
    InvalidRetMapDefinition(String),
    EmptyEntry,
    FlagBeforeCommand(String),
    NoCommands,
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
        }
    }
}
