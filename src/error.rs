use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::sync::atomic::{AtomicBool, Ordering};

/// Wraps an error with optional context.
#[derive(Debug)]
pub struct AsyncError<E: Error + 'static> {
    error: E,
    context: Option<String>,
    hooks_invoked: AtomicBool,
}

impl<E: Error + 'static> AsyncError<E> {
    /// Creates a new error wrapper without context.
    pub fn new(error: E) -> Self {
        Self {
            error,
            context: None,
            hooks_invoked: AtomicBool::new(false),
        }
    }

    /// Adds context to the error.
    ///
    /// If the `hooks` feature is enabled, hooks may be triggered.
    pub fn with_context(mut self, context: String) -> Self {
        self.context = Some(context);
        #[cfg(feature = "hooks")]
        {
            crate::hooks::invoke_hooks(&self);
        }
        self
    }

    /// Returns a reference to the inner error.
    pub fn inner_error(&self) -> &E {
        &self.error
    }

    /// Returns the context string, if any.
    pub fn context(&self) -> Option<&str> {
        self.context.as_deref()
    }

    /// Returns true if hooks have not been invoked yet, and marks them as invoked.
    pub fn invoke_hooks_once(&self) -> bool {
        self.hooks_invoked
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::Relaxed)
            .is_ok()
    }
}

impl<E: Error + 'static> Display for AsyncError<E> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match &self.context {
            Some(ctx) if !ctx.trim().is_empty() => write!(f, "{}: {}", ctx, self.error),
            _ => write!(f, "{}", self.error),
        }
    }
}

impl<E: Error + 'static> Error for AsyncError<E> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.error)
    }
}
