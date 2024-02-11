use std::sync::mpsc;

pub type TogetherResult<T> = std::result::Result<T, TogetherError>;

#[derive(Debug)]
pub enum TogetherError {
    Io(std::io::Error),
    ChannelRecvError(mpsc::RecvError),
    DynError(Box<dyn std::error::Error>),
}

impl std::fmt::Display for TogetherError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            TogetherError::Io(e) => write!(f, "IO error: {}", e),
            TogetherError::ChannelRecvError(e) => write!(f, "Channel receive error: {}", e),
            TogetherError::DynError(e) => write!(f, "Error: {}", e),
        }
    }
}

impl std::error::Error for TogetherError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            TogetherError::Io(e) => Some(e),
            TogetherError::ChannelRecvError(e) => Some(e),
            TogetherError::DynError(e) => Some(e.as_ref()),
        }
    }
}

impl From<std::io::Error> for TogetherError {
    fn from(e: std::io::Error) -> Self {
        TogetherError::Io(e)
    }
}

impl From<mpsc::RecvError> for TogetherError {
    fn from(e: mpsc::RecvError) -> Self {
        TogetherError::ChannelRecvError(e)
    }
}

impl From<Box<dyn std::error::Error>> for TogetherError {
    fn from(e: Box<dyn std::error::Error>) -> Self {
        TogetherError::DynError(e)
    }
}
