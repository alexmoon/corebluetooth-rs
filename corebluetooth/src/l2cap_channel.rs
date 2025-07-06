//! A L2CAP channel for communication between a central and a peripheral.

use std::fmt::Debug;
use std::marker::PhantomData;
use std::os::fd::FromRawFd;
use std::os::unix::net::UnixStream;
use std::os::unix::prelude::RawFd;

use dispatch_executor::{SyncClone, SyncDrop};
use objc2::rc::Retained;
use objc2_core_bluetooth::{CBL2CAPChannel, CBPeer};
use objc2_core_foundation::{CFString, kCFStreamPropertySocketNativeHandle};
use objc2_foundation::{NSData, NSString};

/// A L2CAP channel for communication between a central and a peripheral.
#[derive(Debug)]
pub struct L2capChannel<P> {
    pub(crate) channel: Retained<CBL2CAPChannel>,
    phantom: PhantomData<P>,
}

impl<P> Clone for L2capChannel<P> {
    fn clone(&self) -> Self {
        Self {
            channel: self.channel.clone(),
            phantom: PhantomData,
        }
    }
}

unsafe impl<P> SyncDrop for L2capChannel<P> {}
unsafe impl<P> SyncClone for L2capChannel<P> {}

impl<P: PartialEq> PartialEq for L2capChannel<P> {
    fn eq(&self, other: &Self) -> bool {
        self.channel == other.channel
    }
}

impl<P: Eq> Eq for L2capChannel<P> {}

impl<P: std::hash::Hash> std::hash::Hash for L2capChannel<P> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.channel.hash(state);
    }
}

impl<P> L2capChannel<P> {
    pub(crate) fn new(channel: Retained<CBL2CAPChannel>) -> (Self, UnixStream) {
        // NSStream is toll-free bridged to CFStream which has a
        // [kCFStreamPropertySocketNativeHandle](https://developer.apple.com/documentation/corefoundation/cfstream)
        // property. CFNativeSocketHandle is a file descriptor to a unix socket, compatible with UnixStream.
        let stream = unsafe {
            // Safety: CFStreamPopertyKey is CFString which is toll-free bridged to NSString
            let key = &*(kCFStreamPropertySocketNativeHandle.unwrap() as *const CFString
                as *const NSString);

            // Apple's Swift documentation guarantees these are non-nil
            let input_stream = channel.inputStream().unwrap();
            let output_stream = channel.outputStream().unwrap();

            // L2CAP streams should be backed by native sockets
            let input_native_socket = input_stream.propertyForKey(key).unwrap();
            let output_native_socket = output_stream.propertyForKey(key).unwrap();

            // The value for the kCFStreamPropertySocketNativeHandle is documented to be of type NSData
            let input_native_socket: Retained<NSData> = input_native_socket.downcast().unwrap();
            let output_native_socket: Retained<NSData> = output_native_socket.downcast().unwrap();

            // Both streams should point to the same native socket
            assert_eq!(input_native_socket, output_native_socket);

            // The property value should be a file descriptor
            let fd =
                RawFd::from_ne_bytes(input_native_socket.as_bytes_unchecked().try_into().unwrap());

            UnixStream::from_raw_fd(fd)
        };

        (
            Self {
                channel,
                phantom: PhantomData,
            },
            stream,
        )
    }

    /// The PSM of the L2CAP channel.
    ///
    /// See [`-[CBL2CAPChannel PSM]`](https://developer.apple.com/documentation/corebluetooth/cbl2capchannel/psm).
    pub fn psm(&self) -> u16 {
        unsafe { self.channel.PSM() }
    }

    /// The peer of the L2CAP channel.
    ///
    /// See [`-[CBL2CAPChannel peer]`](https://developer.apple.com/documentation/corebluetooth/cbl2capchannel/peer).
    pub fn peer(&self) -> P
    where
        P: TryFrom<Retained<CBPeer>>,
    {
        match unsafe { self.channel.peer() }.unwrap().try_into() {
            Ok(peer) => peer,
            Err(_) => panic!("Unexpected peer type for L2capChannel"),
        }
    }

    #[doc(hidden)]
    pub fn map<Q: TryFrom<Retained<CBPeer>>>(self) -> L2capChannel<Q> {
        L2capChannel {
            channel: self.channel,
            phantom: PhantomData,
        }
    }
}
