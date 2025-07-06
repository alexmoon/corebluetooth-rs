//! Error types for this crate.

use std::fmt::Display;

use futures_channel::oneshot;
use objc2_core_bluetooth::{CBATTError, CBError};

/// A convenience type alias for a `Result` with an `Error` type.
pub type Result<T> = std::result::Result<T, Error>;

/// An error that can occur in this crate.
#[derive(Debug, Clone)]
pub struct Error {
    data: ErrorData,
}

/// The kind of error that occurred.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ErrorKind {
    /// A Core Bluetooth error.
    Bluetooth(CBError),
    /// A Bluetooth GATT server error.
    ATT(CBATTError),
    /// The operation was canceled.
    Canceled,
    /// A broadcast channel lagged.
    Lagged,
    /// An unknown or other error.
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

impl From<corebluetooth::error::ErrorKind> for Error {
    fn from(kind: corebluetooth::error::ErrorKind) -> Self {
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
    /// If this is an `ErrorData::Os` error, returns a reference to the underlying `corebluetooth::Error`.
    pub fn get_ref(&self) -> Option<&corebluetooth::Error> {
        match &self.data {
            ErrorData::Os(error) => Some(error),
            ErrorData::Simple(_) => None,
        }
    }

    /// If this is an `ErrorData::Os` error, returns the underlying `corebluetooth::Error`.
    pub fn into_inner(self) -> Option<corebluetooth::Error> {
        match self.data {
            ErrorData::Os(error) => Some(error),
            ErrorData::Simple(_) => None,
        }
    }

    /// Returns the kind of error.
    pub fn kind(&self) -> ErrorKind {
        match &self.data {
            ErrorData::Os(error) => error.kind().into(),
            ErrorData::Simple(kind) => *kind,
        }
    }
}

impl From<corebluetooth::error::ErrorKind> for ErrorKind {
    fn from(kind: corebluetooth::error::ErrorKind) -> Self {
        match kind {
            corebluetooth::error::ErrorKind::Bluetooth(cberror) => ErrorKind::Bluetooth(cberror),
            corebluetooth::error::ErrorKind::ATT(cbatterror) => ErrorKind::ATT(cbatterror),
            corebluetooth::error::ErrorKind::Other => ErrorKind::Other,
        }
    }
}

impl TryFrom<ErrorKind> for corebluetooth::error::ErrorKind {
    type Error = ErrorKind;

    fn try_from(kind: ErrorKind) -> std::result::Result<Self, Self::Error> {
        match kind {
            ErrorKind::Bluetooth(cberror) => {
                Ok(corebluetooth::error::ErrorKind::Bluetooth(cberror))
            }
            ErrorKind::ATT(cbatterror) => Ok(corebluetooth::error::ErrorKind::ATT(cbatterror)),
            ErrorKind::Other => Ok(corebluetooth::error::ErrorKind::Other),
            ErrorKind::Canceled => Err(kind),
            ErrorKind::Lagged => Err(kind),
        }
    }
}

impl Display for ErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErrorKind::Bluetooth(cberror) => {
                corebluetooth::error::ErrorKind::Bluetooth(*cberror).fmt(f)
            }
            ErrorKind::ATT(cbatterror) => corebluetooth::error::ErrorKind::ATT(*cbatterror).fmt(f),
            ErrorKind::Other => corebluetooth::error::ErrorKind::Other.fmt(f),
            ErrorKind::Canceled => f.write_str("canceled"),
            ErrorKind::Lagged => f.write_str("lagged"),
        }
    }
}
