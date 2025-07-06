//! An asynchronous executor for Apple's Grand Central Dispatch.
//!
//! This crate provides an [`Executor`] that can be used to spawn and run
//! asynchronous tasks on a GCD dispatch queue.
//!
//! It also provides a [`Handle`] type that allows for sending `!Send` values
//! between threads, as long as they are only accessed on the thread that owns them.
//!
//! # Example
//!
//! ```no_run
//! # use dispatch_executor::{Executor, MainThreadMarker};
//! # async fn example() {
//! let mtm = MainThreadMarker::new().unwrap();
//! let executor = Executor::main_thread(mtm);
//!
//! let task = executor.spawn(async {
//!    println!("Hello, world!");
//!    42
//! });
//!
//! assert_eq!(task.await, 42);
//! # }
//! ```

use std::hash::Hasher;
use std::marker::PhantomData;
use std::mem::MaybeUninit;
use std::pin::Pin;
use std::task::{Context, Poll};

use async_task::{Runnable, spawn, spawn_unchecked};
use dispatch2::{DispatchObject, DispatchRetained};

pub use dispatch2::{DispatchAutoReleaseFrequency, DispatchQoS, DispatchQueue, DispatchQueueAttr};
pub use objc2::MainThreadMarker;

/// An executor that runs async tasks on a Grand Central Dispatch queue.
#[derive(Clone)]
pub struct Executor {
    queue: DispatchRetained<DispatchQueue>,
    phantom: PhantomData<*mut ()>,
}

impl Executor {
    /// Creates a new executor on a background dispatch queue and passes it to the provided entry point function.
    pub fn background<F, R>(
        label: &str,
        queue_attributes: Option<&DispatchQueueAttr>,
        entry: F,
    ) -> R
    where
        F: FnOnce(Self) -> R + Send,
        R: Send,
    {
        let queue = DispatchQueue::new(label, queue_attributes);
        let mut ret = MaybeUninit::uninit();
        queue.barrier_sync(|| {
            let executor = Self {
                queue: queue.retain(),
                phantom: PhantomData,
            };
            ret.write(entry(executor));
        });
        unsafe { ret.assume_init() }
    }

    /// Returns an executor that runs tasks on the main dispatch queue.
    pub fn main_thread(_mtm: MainThreadMarker) -> Self {
        Self {
            queue: DispatchQueue::main().retain(),
            phantom: PhantomData,
        }
    }

    /// Create a [`Handle`] to a value that can be sent between threads.
    ///
    /// `Handle` ensures that all accesses to `value` through the handle are synchronized on this executor's dispatch queue.
    pub fn handle<T>(&self, value: T) -> Handle<T> {
        Handle {
            queue: self.queue.clone(),
            value,
        }
    }

    /// Spawns a new asynchronous task, returning a [`Task`] that can be used to await its result.
    ///
    /// Dropping the `Task` will cancel it. If you want the task to run independently, you must call [`detach()`][Task::detach]
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

    /// Spawns a `!Send` future on the current executor.
    ///
    /// # Safety
    ///
    /// `future` is not required to be `Send`, but must not have any thread affinity unless this is
    /// the main thread executor. Grand Central Dispatch will coordinate the execution of `future`
    /// synchronously with respect to other tasks on the same queue, but it may run on different
    /// threads. `future` must not access any thread-local resources or otherwise depend on the
    /// specific thread id it is running on.
    pub unsafe fn spawn_local<R>(&self, future: impl Future<Output = R> + 'static) -> Task<R>
    where
        R: 'static,
    {
        let queue = self.queue.clone();
        let (runnable, task) = unsafe {
            // Safety: Because `Executor` is `!Send` we know that any `!Send` values inside `future`
            // are accessible only within the context of our dispatch queue. Because `barrier_async`
            // synchronizes all access to the runnable exclusively within the dispatch queue, there
            // is no possibility of data races between the `runnable` and any other references to
            // values within the future.
            spawn_unchecked(future, move |runnable: Runnable| {
                queue.barrier_async(|| {
                    runnable.run();
                })
            })
        };
        runnable.schedule();
        Task(TaskState::Spawned(task))
    }

    /// Returns a reference to the underlying [`DispatchQueue`].
    pub fn queue(&self) -> &DispatchQueue {
        &self.queue
    }
}

/// A marker trait for values whose `Drop` implementation is `Sync`.
///
/// These values can be moved across threads even if they are `!Send`
/// as long as they are only accessed from their native thread.
///
/// # Safety
///
/// It must be safe to drop values of this type from arbitrary threads.
pub unsafe trait SyncDrop {}

/// A marker trait for values whose `Clone` implementation is `Sync`.
///
/// These values can be cloned on another thread even if they are `!Sync`.
/// For example, Objective-C pointers can be retained synchronously even
/// when the underlying object is `!Sync`.
///
/// # Safety
///
/// It must be safe to clone values of this type from arbitrary threads.
pub unsafe trait SyncClone: Clone {}

unsafe impl<T> SyncDrop for &T {}
unsafe impl<T> SyncClone for &T {}

unsafe impl<T: SyncDrop, U: SyncDrop> SyncDrop for (T, U) {}
unsafe impl<T: SyncClone, U: SyncClone> SyncClone for (T, U) {}

/// A handle to a value that is owned by a specific [`Executor`].
///
/// This allows for sending `!Send` values between threads, as long as they are only
/// accessed on the thread that owns them.
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
    /// Acquires a lock on the value, running the provided function on the owning executor's dispatch queue.
    ///
    /// This method will block the current thread until the function returns.
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

    /// Zips two handles together, creating a new handle that provides access to both values.
    ///
    /// # Panics
    ///
    /// This method will panic if the two handles are not from the same executor.
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
            phantom: PhantomData,
        }
    }
}

#[derive(Debug)]
enum TaskState<T> {
    Ready(Option<T>),
    Spawned(async_task::Task<T>),
}

/// A future that resolves to the result of an asynchronous task.
///
/// Dropping a [`Task`] cancels it, which means its future won't be polled again. To drop the
/// [`Task`] handle without canceling it, use [`detach()`][`Task::detach()`] instead.
pub struct Task<T>(TaskState<T>);

impl<T> Task<T> {
    /// Creates a new task that is already completed with the given value.
    pub fn ready(val: T) -> Self {
        Task(TaskState::Ready(Some(val)))
    }

    /// Detaches the task, allowing it to run in the background.
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

/// A builder for creating [`DispatchQueueAttr`] objects.
#[derive(Debug, Clone)]
pub struct DispatchQueueAttrBuilder {
    attr: Option<DispatchRetained<DispatchQueueAttr>>,
}

impl DispatchQueueAttrBuilder {
    /// Creates a new builder for a serial dispatch queue.
    pub fn serial() -> Self {
        Self { attr: None }
    }

    /// Creates a new builder for a concurrent dispatch queue.
    pub fn concurrent() -> Self {
        Self {
            attr: DispatchQueueAttr::concurrent().map(|x| x.retain()),
        }
    }

    /// Sets the autorelease frequency for the dispatch queue.
    pub fn with_autorelease_frequency(mut self, frequency: DispatchAutoReleaseFrequency) -> Self {
        self.attr = Some(DispatchQueueAttr::with_autorelease_frequency(
            self.attr.as_deref(),
            frequency,
        ));
        self
    }

    /// Sets the quality-of-service class and relative priority for the dispatch queue.
    pub fn with_qos_class(mut self, qos_class: DispatchQoS, relative_priority: i32) -> Self {
        self.attr = Some(DispatchQueueAttr::with_qos_class(
            self.attr.as_deref(),
            qos_class,
            relative_priority,
        ));
        self
    }

    /// Builds the [`DispatchQueueAttr`] object.
    pub fn build(self) -> Option<DispatchRetained<DispatchQueueAttr>> {
        self.attr
    }
}
