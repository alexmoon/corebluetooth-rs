use std::fmt::Display;

use objc2::Message;
use objc2::rc::Retained;
use objc2_core_bluetooth::{CBATTError, CBATTErrorDomain, CBError, CBErrorDomain};
use objc2_foundation::NSError;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone)]
pub struct Error {
    data: ErrorData,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ErrorKind {
    Bluetooth(CBError),
    ATT(CBATTError),
    Other,
}

#[derive(Debug, Clone)]
enum ErrorData {
    Os(Retained<NSError>),
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

impl From<ErrorKind> for Error {
    fn from(kind: ErrorKind) -> Self {
        Error {
            data: ErrorData::Simple(kind),
        }
    }
}

impl Error {
    pub(crate) fn from_nserror(error: &NSError) -> Self {
        Self {
            data: ErrorData::Os(error.retain()),
        }
    }

    pub(crate) fn from_nserror_or_kind(error: Option<&NSError>, kind: ErrorKind) -> Self {
        if let Some(error) = error {
            Self::from_nserror(error)
        } else {
            Self {
                data: ErrorData::Simple(kind),
            }
        }
    }

    pub fn get_ref(&self) -> Option<&NSError> {
        match &self.data {
            ErrorData::Os(error) => Some(error),
            ErrorData::Simple(_) => None,
        }
    }

    pub fn into_inner(self) -> Option<Retained<NSError>> {
        match self.data {
            ErrorData::Os(error) => Some(error),
            ErrorData::Simple(_) => None,
        }
    }

    pub fn kind(&self) -> ErrorKind {
        match &self.data {
            ErrorData::Os(error) => ErrorKind::from(&**error),
            ErrorData::Simple(kind) => *kind,
        }
    }
}

impl From<&NSError> for ErrorKind {
    fn from(error: &NSError) -> Self {
        if &*error.domain() == unsafe { CBErrorDomain } {
            ErrorKind::Bluetooth(CBError(error.code()))
        } else if &*error.domain() == unsafe { CBATTErrorDomain } {
            ErrorKind::ATT(CBATTError(error.code()))
        } else {
            ErrorKind::Other
        }
    }
}

impl Display for ErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErrorKind::Bluetooth(cb_error) => match *cb_error {
                CBError::Unknown => f.write_str("unknown"),
                CBError::InvalidParameters => f.write_str("invalid parameters"),
                CBError::InvalidHandle => f.write_str("invalid handle"),
                CBError::NotConnected => f.write_str("not connected"),
                CBError::OutOfSpace => f.write_str("out of space"),
                CBError::OperationCancelled => f.write_str("operation cancelled"),
                CBError::ConnectionTimeout => f.write_str("connection timeout"),
                CBError::PeripheralDisconnected => f.write_str("peripheral disconnected"),
                CBError::UUIDNotAllowed => f.write_str("UUID not allowed"),
                CBError::AlreadyAdvertising => f.write_str("already advertising"),
                CBError::ConnectionFailed => f.write_str("connection failed"),
                CBError::ConnectionLimitReached => f.write_str("connection limit reached"),
                CBError::UnknownDevice => f.write_str("unknown device"),
                CBError::OperationNotSupported => f.write_str("operation not supported"),
                CBError::PeerRemovedPairingInformation => {
                    f.write_str("peer removed pairing information")
                }
                CBError::EncryptionTimedOut => f.write_str("encryption timed out"),
                CBError::TooManyLEPairedDevices => f.write_str("too many LE paired devices"),
                CBError::LeGattExceededBackgroundNotificationLimit => {
                    f.write_str("LE GATT exceeded background notification limit")
                }
                CBError::LeGattNearBackgroundNotificationLimit => {
                    f.write_str("LE GATT near background notification limit")
                }
                _ => write!(f, "unknown bluetooth error ({})", cb_error.0),
            },
            ErrorKind::ATT(cb_att_error) => match *cb_att_error {
                CBATTError::Success => f.write_str("success"),
                CBATTError::InvalidHandle => f.write_str("invalid handle"),
                CBATTError::ReadNotPermitted => f.write_str("read not permitted"),
                CBATTError::WriteNotPermitted => f.write_str("write not permitted"),
                CBATTError::InvalidPdu => f.write_str("invalid PDU"),
                CBATTError::InsufficientAuthentication => {
                    f.write_str("insufficient authentication")
                }
                CBATTError::RequestNotSupported => f.write_str("request not supported"),
                CBATTError::InvalidOffset => f.write_str("invalid offset"),
                CBATTError::InsufficientAuthorization => f.write_str("insufficient authorization"),
                CBATTError::PrepareQueueFull => f.write_str("prepare queue full"),
                CBATTError::AttributeNotFound => f.write_str("attribute not found"),
                CBATTError::AttributeNotLong => f.write_str("attribute not long"),
                CBATTError::InsufficientEncryptionKeySize => {
                    f.write_str("insufficient encryption key size")
                }
                CBATTError::InvalidAttributeValueLength => {
                    f.write_str("invalid attribute value length")
                }
                CBATTError::UnlikelyError => f.write_str("unlikely error"),
                CBATTError::InsufficientEncryption => f.write_str("insufficient encryption"),
                CBATTError::UnsupportedGroupType => f.write_str("unsupported group type"),
                CBATTError::InsufficientResources => f.write_str("insufficient resources"),
                _ => write!(f, "unknown bluetooth ATT error ({})", cb_att_error.0),
            },
            ErrorKind::Other => f.write_str("other error"),
        }
    }
}
