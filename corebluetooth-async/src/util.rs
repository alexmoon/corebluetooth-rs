use std::mem::ManuallyDrop;
use std::ops::{Deref, DerefMut};

pub struct ScopeGuard<F: FnOnce()> {
    dropfn: ManuallyDrop<F>,
}

impl<F: FnOnce()> ScopeGuard<F> {
    pub fn defuse(mut self) {
        unsafe { ManuallyDrop::drop(&mut self.dropfn) }
        std::mem::forget(self)
    }
}

impl<F: FnOnce()> Drop for ScopeGuard<F> {
    fn drop(&mut self) {
        // SAFETY: This is OK because `dropfn` is `ManuallyDrop` which will not be dropped by the compiler.
        let dropfn = unsafe { ManuallyDrop::take(&mut self.dropfn) };
        dropfn();
    }
}

pub fn defer<F: FnOnce()>(dropfn: F) -> ScopeGuard<F> {
    ScopeGuard {
        dropfn: ManuallyDrop::new(dropfn),
    }
}

pub struct BroadcastSender<T> {
    sender: async_broadcast::Sender<T>,
    _keep_alive: async_broadcast::InactiveReceiver<T>,
}

impl<T> Deref for BroadcastSender<T> {
    type Target = async_broadcast::Sender<T>;

    fn deref(&self) -> &Self::Target {
        &self.sender
    }
}

impl<T> DerefMut for BroadcastSender<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.sender
    }
}

pub type BroadcastReceiver<T> = async_broadcast::Receiver<T>;

pub fn broadcast<T>(cap: usize) -> BroadcastSender<T> {
    let (mut sender, receiver) = async_broadcast::broadcast(cap);
    sender.set_overflow(true);
    BroadcastSender {
        sender,
        _keep_alive: receiver.deactivate(),
    }
}

pub fn watch<T>() -> BroadcastSender<T> {
    broadcast(1)
}
