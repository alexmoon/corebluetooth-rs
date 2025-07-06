# dispatch-executor

An asynchronous executor for Apple's Grand Central Dispatch.

This crate provides an `Executor` that can be used to spawn and run
asynchronous tasks on a GCD dispatch queue.

It also provides a `Handle` type that allows for sending `!Send` values
between threads, as long as they are only accessed on the thread that owns them.

## Example

```rust,no_run
use dispatch_executor::{Executor, MainThreadMarker};

async fn example() {
    let mtm = MainThreadMarker::new().unwrap();
    let executor = Executor::main_thread(mtm);

    let task = executor.spawn(async {
        println!("Hello, world!");
        42
    });

    assert_eq!(task.await, 42);
}
```
