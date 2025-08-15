#![cfg(feature = "hooks")]
use crate::AsyncError;
use downcast_rs::{impl_downcast, DowncastSync};
use once_cell::sync::Lazy;
use parking_lot::RwLock;
use std::{
    any::TypeId,
    collections::HashMap,
    error::Error,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
};

static TIMESTAMP_ENABLED: AtomicBool = AtomicBool::new(false);

/// Enable timestamped hook output globally.
///
/// When enabled, async error hooks will include a timestamp prefix in their output.
/// This can help correlate error logs with event times.
pub fn enable_hook_timestamps() {
    TIMESTAMP_ENABLED.store(true, Ordering::SeqCst);
}

/// Disable timestamped hook output globally.
///
/// Disabling timestamps causes hook output to omit the time prefix,
/// producing simpler logs.
pub fn disable_hook_timestamps() {
    TIMESTAMP_ENABLED.store(false, Ordering::SeqCst);
}

/// Trait representing hooks that run on async errors, supporting downcasting.
///
/// # Type parameters
///
/// - `E`: The error type this hook handles.
///
/// Implementors can hook into async error occurrences for logging,
/// metrics, notifications, or other side effects.
///
/// Hooks must be thread-safe (`Send + Sync`) and `'static`.
pub trait AsyncErrorHook<E: Error + 'static>: Send + Sync + 'static + DowncastSync {
    /// Called when an async error of type `E` is encountered.
    ///
    /// The `error` parameter provides access to the error and its context.
    fn on_error(&self, error: &AsyncError<E>);
}

impl_downcast!(sync AsyncErrorHook<E> where E: Error + 'static);

/// Provides a default implementation of `on_error` to simplify common hooks.
///
/// Typical usage is for hooks that want to log errors with optional timestamps without
/// needing to explicitly implement the `on_error` method themselves.
///
/// This trait is automatically implemented for all types implementing `AsyncErrorHook`.
pub trait AsyncErrorHookDefault<E: Error + 'static>: AsyncErrorHook<E> {
    /// Default `on_error` implementation prints a timestamped message showing
    /// the error context and inner error details.
    fn on_error(&self, error: &AsyncError<E>) {
        let header = if TIMESTAMP_ENABLED.load(Ordering::SeqCst) {
            #[cfg(feature = "chrono")]
            {
                use chrono::Local;
                let now = Local::now();
                format!(
                    "{} | AsyncError Hook Triggered",
                    now.format("%Y-%m-%d %H:%M:%S")
                )
            }
            #[cfg(not(feature = "chrono"))]
            {
                let now = std::time::SystemTime::now();
                match now.duration_since(std::time::UNIX_EPOCH) {
                    Ok(dur) => format!("[{}] | AsyncError Hook Triggered", dur.as_secs()),
                    Err(_) => "[time unknown] | AsyncError Hook Triggered".to_string(),
                }
            }
        } else {
            "AsyncError Hook Triggered".to_string()
        };
        let context = error.context().unwrap_or("<none>");
        let msg = format!(
            "{}\n  Context: {}\n  Inner error: {}\n------------------------------",
            header,
            context,
            error.inner_error()
        );
        eprintln!("{}", msg);
    }
}

impl<E: Error + 'static, T> AsyncErrorHookDefault<E> for T where T: AsyncErrorHook<E> {}

/// Internal registry storing hooks for a specific error type `E`.
struct HookRegistry<E: Error + 'static> {
    hooks: Vec<Arc<dyn AsyncErrorHook<E>>>,
}

static GLOBAL_HOOKS: Lazy<RwLock<HashMap<TypeId, Box<dyn std::any::Any + Send + Sync>>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

/// Register a new hook for a specific error type `E`.
///
/// Multiple hooks can be registered for the same error type.
/// Duplicate registrations (same hook instance) are ignored.
///
/// # Parameters
///
/// - `hook`: An `Arc`-wrapped hook instance implementing `AsyncErrorHook<E>`.
///
/// # Notes
///
/// This function requires explicit generic type annotation for `E` to clarify the error type.
pub fn register_hook<E: Error + 'static>(hook: Arc<dyn AsyncErrorHook<E>>) {
    let mut registry = GLOBAL_HOOKS.write();
    let type_id = TypeId::of::<E>();
    let entry = registry
        .entry(type_id)
        .or_insert_with(|| Box::new(HookRegistry::<E> { hooks: Vec::new() }));
    let hooks = entry
        .downcast_mut::<HookRegistry<E>>()
        .expect("Type mismatch in global hooks registry");
    if !hooks
        .hooks
        .iter()
        .any(|existing| Arc::ptr_eq(existing, &hook))
    {
        hooks.hooks.push(hook);
    }
}

/// Retrieve all registered hooks for the specified error type `E`.
///
/// Hooks are returned cloned as `Arc` references.
///
/// # Returns
///
/// A vector of `Arc`-wrapped hooks. If no hooks are registered for `E`, returns an empty vector.
pub fn get_hooks<E: Error + 'static>() -> Vec<Arc<dyn AsyncErrorHook<E>>> {
    let registry = GLOBAL_HOOKS.read();
    registry
        .get(&TypeId::of::<E>())
        .and_then(|entry| entry.downcast_ref::<HookRegistry<E>>())
        .map(|hooks| hooks.hooks.clone())
        .unwrap_or_default()
}

static HOOK_INVOKE_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Invoke all registered hooks for this error, ensuring only one concurrent invocation.
///
/// Concurrent duplicate invocations are guarded by an atomic compare-and-swap counter,
/// so only the first caller runs hooks, others return early.
///
/// # Parameters
///
/// - `error`: Reference to the async error triggering hooks.
///
/// # Notes
///
/// This method does not prevent sequential calls from multiple threads at different times.
pub fn invoke_hooks<E: Error + 'static>(error: &AsyncError<E>) {
    // Attempt to set counter from 0 to 1 atomically; if already set, skip invocation
    if HOOK_INVOKE_COUNTER
        .compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        return;
    }
    for hook in get_hooks::<E>() {
        hook.on_error(error);
    }
    HOOK_INVOKE_COUNTER.store(0, Ordering::Release);
}
