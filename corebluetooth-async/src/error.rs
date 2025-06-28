use std::fmt::Display;

use futures_channel::oneshot;
use objc2_core_bluetooth::{CBATTError, CBError};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone)]
pub struct Error {
    data: ErrorData,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ErrorKind {
    Bluetooth(CBError),
    ATT(CBATTError),
    Canceled,
    Lagged,
    Other,
}

#[derive(Debug, Clone)]
enum ErrorData {
    Os(corebluetooth::Error),
    Simple(ErrorKind),
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.data {
            ErrorData::Os(error) => error.fmt(f),
            ErrorData::Simple(kind) => kind.fmt(f),
        }
    }
}

impl std::error::Error for Error {}

impl From<corebluetooth::Error> for Error {
    fn from(error: corebluetooth::Error) -> Self {
        Error {
            data: ErrorData::Os(error),
        }
    }
}

impl From<ErrorKind> for Error {
    fn from(kind: ErrorKind) -> Self {
        Error {
            data: ErrorData::Simple(kind),
        }
    }
}

impl From<corebluetooth::ErrorKind> for Error {
    fn from(kind: corebluetooth::ErrorKind) -> Self {
        Error {
            data: ErrorData::Simple(kind.into()),
        }
    }
}

impl From<oneshot::Canceled> for Error {
    fn from(_value: oneshot::Canceled) -> Self {
        ErrorKind::Canceled.into()
    }
}

impl From<async_broadcast::RecvError> for Error {
    fn from(_value: async_broadcast::RecvError) -> Self {
        ErrorKind::Lagged.into()
    }
}

impl Error {
    pub fn get_ref(&self) -> Option<&corebluetooth::Error> {
        match &self.data {
            ErrorData::Os(error) => Some(error),
            ErrorData::Simple(_) => None,
        }
    }

    pub fn into_inner(self) -> Option<corebluetooth::Error> {
        match self.data {
            ErrorData::Os(error) => Some(error),
            ErrorData::Simple(_) => None,
        }
    }

    pub fn kind(&self) -> ErrorKind {
        match &self.data {
            ErrorData::Os(error) => error.kind().into(),
            ErrorData::Simple(kind) => *kind,
        }
    }
}

impl From<corebluetooth::ErrorKind> for ErrorKind {
    fn from(kind: corebluetooth::ErrorKind) -> Self {
        match kind {
            corebluetooth::ErrorKind::Bluetooth(cberror) => ErrorKind::Bluetooth(cberror),
            corebluetooth::ErrorKind::ATT(cbatterror) => ErrorKind::ATT(cbatterror),
            corebluetooth::ErrorKind::Other => ErrorKind::Other,
        }
    }
}

impl TryFrom<ErrorKind> for corebluetooth::ErrorKind {
    type Error = ErrorKind;

    fn try_from(kind: ErrorKind) -> std::result::Result<Self, Self::Error> {
        match kind {
            ErrorKind::Bluetooth(cberror) => Ok(corebluetooth::ErrorKind::Bluetooth(cberror)),
            ErrorKind::ATT(cbatterror) => Ok(corebluetooth::ErrorKind::ATT(cbatterror)),
            ErrorKind::Other => Ok(corebluetooth::ErrorKind::Other),
            ErrorKind::Canceled => Err(kind),
            ErrorKind::Lagged => Err(kind),
        }
    }
}

impl Display for ErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErrorKind::Bluetooth(cberror) => corebluetooth::ErrorKind::Bluetooth(*cberror).fmt(f),
            ErrorKind::ATT(cbatterror) => corebluetooth::ErrorKind::ATT(*cbatterror).fmt(f),
            ErrorKind::Other => corebluetooth::ErrorKind::Other.fmt(f),
            ErrorKind::Canceled => f.write_str("canceled"),
            ErrorKind::Lagged => f.write_str("lagged"),
        }
    }
}
