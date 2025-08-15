# async-err

Contextual async error handling for Rust.

## Overview

`async-err` provides utilities for adding rich, contextual information to asynchronous errors in Rust. It offers:

- A wrapper error type `AsyncError` that attaches lazy, user-defined context strings to any error.
- Extension traits for async futures that allow chaining and adding context easily.
- A hook system to log or react to async errors globally.
- Optional timestamps in hook logs with `chrono` integration.

## Features

- **Error Wrapping:** Wrap errors with extra context captured lazily.
- **Async Extensions:** `.with_context()` and `.and_then_async()` combinators for ergonomic async error flows.
- **Hooks:** Register global hooks to log or process errors when they occur.
- **Configurable Timestamping:** Optional timestamps with `chrono` feature support.

## Usage

Add `async-err` to your dependencies:

```rust
async-err = { version = "0.1", features = ["hooks", "chrono"] }
```

- The `hooks` feature enables the global hooks system for async error logging and processing.
- The `chrono` feature adds timestamp support in hooks output via the `chrono` crate.

Example usage in async code:

```rust
#[cfg(feature = "hooks")]
use async_err::hooks::{enable_hook_timestamps, register_hook, AsyncErrorHookDefault};
use async_err::prelude::*;
use std::{io, sync::Arc};

struct LoggingHook;

#[cfg(feature = "hooks")]
impl AsyncErrorHook<io::Error> for LoggingHook {
    fn on_error(&self, error: &AsyncError<io::Error>) {
        <Self as AsyncErrorHookDefault<io::Error>>::on_error(self, error);
    }
}

async fn step1(val: i32) -> Result<i32, io::Error> {
    Ok(val + 1)
}

async fn step2(val: i32) -> Result<i32, io::Error> {
    if val % 2 == 0 {
        Ok(val * 2)
    } else {
        Err(io::Error::other("Odd value at step2"))
    }
}

async fn step3(val: i32) -> Result<i32, io::Error> {
    if val < 10 {
        Ok(val + 5)
    } else {
        Err(io::Error::other("Value too large at step3"))
    }
}

#[tokio::main]
async fn main() -> Result<(), AsyncError<io::Error>> {
    #[cfg(feature = "hooks")]
    {
        register_hook::<io::Error>(Arc::new(LoggingHook));
        enable_hook_timestamps();
    }

    let result = step1(2)
        .with_context(|_| "Failed at step1".to_string())
        .and_then_async(|v| step2(v).with_context(|_| "Failed at step2".to_string()))
        .and_then_async(|v| step3(v).with_context(|_| "Failed at step3".to_string()))
        .await;

    match &result {
        Ok(val) => println!("Success! Result: {}", val),
        Err(e) => {
            eprintln!("Error occurred:");
            if let Some(ctx) = e.context() {
                eprintln!("  Context: {}", ctx);
            }
            eprintln!("  Error: {}", e.inner_error());
        }
    }

    result.map(|_| ())
}
```