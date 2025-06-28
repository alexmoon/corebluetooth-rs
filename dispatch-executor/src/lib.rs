use std::hash::Hasher;
use std::mem::MaybeUninit;
use std::pin::Pin;
use std::task::{Context, Poll};

use async_task::{Runnable, spawn, spawn_unchecked};
use dispatch2::{
    DispatchAutoReleaseFrequency, DispatchObject, DispatchQoS, DispatchQueue, DispatchQueueAttr,
    DispatchRetained,
};

#[derive(Clone)]
pub struct Executor {
    queue: DispatchRetained<DispatchQueue>,
}

impl Executor {
    pub fn background<F, R>(label: &str, queue_attributes: Option<&DispatchQueueAttr>, func: F) -> R
    where
        F: FnOnce(Self) -> R + Send,
        R: Send,
    {
        let queue = DispatchQueue::new(label, queue_attributes);
        let mut ret = MaybeUninit::uninit();
        queue.barrier_sync(|| {
            let executor = Self {
                queue: queue.retain(),
            };
            ret.write(func(executor));
        });
        unsafe { ret.assume_init() }
    }

    pub fn main_thread() -> Self {
        Self {
            queue: DispatchQueue::main().retain(),
        }
    }

    /// Create a [`Handle`] to a value that can be sent between threads.
    ///
    /// Ensures that all accesses to `value` through the handle are synchronized on this executor's dispatch queue.
    pub fn handle<T>(&self, value: T) -> Handle<T> {
        Handle {
            queue: self.queue.clone(),
            value,
        }
    }

    pub fn spawn<R>(&self, future: impl Future<Output = R> + Send + 'static) -> Task<R>
    where
        R: Send + 'static,
    {
        let queue = self.queue.clone();
        let (runnable, task) = spawn(future, move |runnable: Runnable| {
            queue.exec_async(|| {
                runnable.run();
            })
        });
        runnable.schedule();
        Task(TaskState::Spawned(task))
    }

    pub fn spawn_local<R>(&self, future: impl Future<Output = R> + 'static) -> Task<R>
    where
        R: 'static,
    {
        let queue = self.queue.clone();
        let (runnable, task) = unsafe {
            spawn_unchecked(future, move |runnable: Runnable| {
                queue.barrier_async(|| {
                    runnable.run();
                })
            })
        };
        runnable.schedule();
        Task(TaskState::Spawned(task))
    }

    pub fn queue(&self) -> &DispatchQueue {
        &self.queue
    }
}

/// A marker trait for values whose `drop` implementation is `Sync`.
///
/// These values can be moved across threads even if they are `!Send`
/// as long as they are only accessed from their native thread.
///
/// # Safety
///
/// It must be safe to drop values of this type from arbitrary threads.
pub unsafe trait SyncDrop {}

/// A marker trait for values whose `clone` implementation is `Sync`.
///
/// These values can be moved across threads even if they are `!Send`
/// as long as they are only accessed from their native thread.
///
/// # Safety
///
/// It must be safe to clone values of this type from arbitrary threads.
pub unsafe trait SyncClone: Clone {}

unsafe impl<T> SyncDrop for &T {}
unsafe impl<T> SyncClone for &T {}

unsafe impl<T: SyncDrop, U: SyncDrop> SyncDrop for (T, U) {}
unsafe impl<T: SyncClone, U: SyncClone> SyncClone for (T, U) {}

pub struct Handle<T> {
    queue: DispatchRetained<DispatchQueue>,
    value: T,
}

unsafe impl<T: SyncDrop> Send for Handle<T> {}

unsafe impl<T: SyncDrop> Sync for Handle<T> {}

impl<T: std::fmt::Debug> std::fmt::Debug for Handle<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("corebluetooth::Handle { .. }")
    }
}

impl<T: SyncClone> Clone for Handle<T> {
    fn clone(&self) -> Self {
        Self {
            queue: self.queue.clone(),
            value: self.value.clone(),
        }
    }
}

impl<T: PartialEq + SyncDrop> PartialEq for Handle<T> {
    fn eq(&self, other: &Self) -> bool {
        self.queue == other.queue && self.lock(|value, _| value == &other.value)
    }
}

impl<T: Eq + SyncDrop> Eq for Handle<T> {}

impl<T: std::hash::Hash + SyncDrop> std::hash::Hash for Handle<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.queue.hash(state);
        self.lock(|value, _| {
            let mut state = std::hash::DefaultHasher::new();
            value.hash(&mut state);
            state.finish()
        })
        .hash(state);
    }
}

impl<T> Handle<T> {
    pub fn lock<R>(&self, func: impl FnOnce(&T, &Executor) -> R + Send) -> R
    where
        Self: Sync,
        R: Send,
    {
        let mut ret = MaybeUninit::uninit();
        self.queue.barrier_sync(|| {
            ret.write(func(&self.value, &self.executor()));
        });
        unsafe { ret.assume_init() }
    }

    pub fn zip<'a, U>(&'a self, other: &'a Handle<U>) -> Handle<(&'a T, &'a U)> {
        assert_eq!(self.queue, other.queue);
        Handle {
            queue: self.queue.clone(),
            value: (&self.value, &other.value),
        }
    }

    fn executor(&self) -> Executor {
        Executor {
            queue: self.queue.clone(),
        }
    }
}

#[derive(Debug)]
enum TaskState<T> {
    Ready(Option<T>),
    Spawned(async_task::Task<T>),
}

pub struct Task<T>(TaskState<T>);

impl<T> Task<T> {
    pub fn ready(val: T) -> Self {
        Task(TaskState::Ready(Some(val)))
    }

    pub fn detach(self) {
        match self {
            Task(TaskState::Ready(_)) => (),
            Task(TaskState::Spawned(task)) => task.detach(),
        }
    }
}

impl<T> Future for Task<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        match unsafe { self.get_unchecked_mut() } {
            Task(TaskState::Ready(val)) => Poll::Ready(val.take().unwrap()),
            Task(TaskState::Spawned(task)) => Pin::new(task).poll(cx),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DispatchQueueAttrBuilder {
    attr: Option<DispatchRetained<DispatchQueueAttr>>,
}

impl DispatchQueueAttrBuilder {
    pub fn serial() -> Self {
        Self { attr: None }
    }

    pub fn concurrent() -> Self {
        Self {
            attr: DispatchQueueAttr::concurrent().map(|x| x.retain()),
        }
    }

    pub fn with_autorelease_frequency(mut self, frequency: DispatchAutoReleaseFrequency) -> Self {
        self.attr = Some(DispatchQueueAttr::with_autorelease_frequency(
            self.attr.as_deref(),
            frequency,
        ));
        self
    }

    pub fn with_qos_class(mut self, qos_class: DispatchQoS, relative_priority: i32) -> Self {
        self.attr = Some(DispatchQueueAttr::with_qos_class(
            self.attr.as_deref(),
            qos_class,
            relative_priority,
        ));
        self
    }

    pub fn build(self) -> Option<DispatchRetained<DispatchQueueAttr>> {
        self.attr
    }
}
