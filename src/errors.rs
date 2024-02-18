use std::sync::mpsc;

pub type TogetherResult<T> = std::result::Result<T, TogetherError>;

#[derive(Debug)]
pub enum TogetherError {
    Io(std::io::Error),
    TomlSerialize(toml::ser::Error),
    TomlDeserialize(toml::de::Error),
    ChannelRecvError(mpsc::RecvError),
    PopenErrorError(subprocess::PopenError),
    InternalError(TogetherInternalError),
    DynError(Box<dyn std::error::Error>),
}

#[derive(Debug)]
pub enum TogetherInternalError {
    ProcessFailedToExit,
    UnexpectedResponse,
}

impl std::fmt::Display for TogetherError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        use TogetherInternalError as TIE;
        match self {
            TogetherError::Io(e) => write!(f, "IO error: {}", e),
            TogetherError::TomlSerialize(e) => write!(f, "TOML serialization error: {}", e),
            TogetherError::TomlDeserialize(e) => write!(f, "TOML deserialization error: {}", e),
            TogetherError::ChannelRecvError(e) => write!(f, "Channel receive error: {}", e),
            TogetherError::PopenErrorError(e) => write!(f, "Process error: {}", e),
            TogetherError::InternalError(TIE::ProcessFailedToExit) => {
                write!(f, "Process failed to exit")
            }
            TogetherError::InternalError(TIE::UnexpectedResponse) => {
                write!(f, "Unexpected response from process")
            }
            TogetherError::DynError(e) => write!(f, "Error: {}", e),
        }
    }
}

impl std::error::Error for TogetherError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            TogetherError::Io(e) => Some(e),
            TogetherError::TomlSerialize(e) => Some(e),
            TogetherError::TomlDeserialize(e) => Some(e),
            TogetherError::ChannelRecvError(e) => Some(e),
            TogetherError::PopenErrorError(e) => Some(e),
            TogetherError::InternalError(_) => None,
            TogetherError::DynError(e) => Some(e.as_ref()),
        }
    }
}

impl From<std::io::Error> for TogetherError {
    fn from(e: std::io::Error) -> Self {
        TogetherError::Io(e)
    }
}

impl From<toml::ser::Error> for TogetherError {
    fn from(e: toml::ser::Error) -> Self {
        TogetherError::TomlSerialize(e)
    }
}

impl From<toml::de::Error> for TogetherError {
    fn from(e: toml::de::Error) -> Self {
        TogetherError::TomlDeserialize(e)
    }
}

impl From<mpsc::RecvError> for TogetherError {
    fn from(e: mpsc::RecvError) -> Self {
        TogetherError::ChannelRecvError(e)
    }
}

impl From<subprocess::PopenError> for TogetherError {
    fn from(e: subprocess::PopenError) -> Self {
        TogetherError::PopenErrorError(e)
    }
}

impl From<TogetherInternalError> for TogetherError {
    fn from(e: TogetherInternalError) -> Self {
        TogetherError::InternalError(e)
    }
}

impl From<Box<dyn std::error::Error>> for TogetherError {
    fn from(e: Box<dyn std::error::Error>) -> Self {
        TogetherError::DynError(e)
    }
}
