use std::error::Error;
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Extension trait providing a `.with_context()` method for futures resolving to `Result<T, E>`.
///
/// This method allows attaching additional context to errors lazily, by supplying
/// a closure that is only executed if the future resolves to an error.
///
/// # Example
/// ```
/// some_async_fn()
///     .with_context(|err| format!("Failed due to: {}", err))
///     .await;
/// ```
pub trait AsyncResultExt<T, E>: Future<Output = Result<T, E>> + Sized {
    /// Adds context to an error produced by this future lazily.
    ///
    /// The closure `ctx` is called only if the future resolves to an error, producing
    /// a string context to be attached to the error.
    ///
    /// # Parameters
    /// - `ctx`: closure to create context string from error reference
    ///
    /// # Returns
    /// A future that resolves to `Result<T, AsyncError<E>>`, where errors are wrapped to include context.
    fn with_context<C>(self, ctx: C) -> WithContext<Self, E, C>
    where
        C: FnOnce(&E) -> String,
    {
        WithContext {
            future: self,
            context: Some(ctx),
            _marker: PhantomData,
        }
    }
}

impl<T, E, Fut> AsyncResultExt<T, E> for Fut where Fut: Future<Output = Result<T, E>> + Sized {}

/// Future wrapper produced by `.with_context()` to add error context.
///
/// Wraps the original future, and on error, attaches the context string lazily generated
/// by the stored closure.
pub struct WithContext<Fut, E, C> {
    future: Fut,
    context: Option<C>,
    _marker: PhantomData<E>,
}

impl<Fut, T, E, C> Future for WithContext<Fut, E, C>
where
    Fut: Future<Output = Result<T, E>>,
    E: Error + 'static,
    C: FnOnce(&E) -> String,
{
    type Output = Result<T, crate::error::AsyncError<E>>;

    /// Polls the wrapped future, converting any error by adding context.
    ///
    /// If the wrapped future resolves to `Ok`, passes the value through.
    /// If `Err`, applies the context closure, wraps the error (without invoking hooks!).
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // Safety: projected pinned fields can be safely accessed
        let this = unsafe { self.get_unchecked_mut() };
        let fut = unsafe { Pin::new_unchecked(&mut this.future) };

        match fut.poll(cx) {
            Poll::Ready(Ok(val)) => Poll::Ready(Ok(val)),
            Poll::Ready(Err(err)) => {
                let ctx = this.context.take().map(|f| f(&err));
                let wrapped =
                    crate::error::AsyncError::new(err).with_context(ctx.unwrap_or_default());

                // Do NOT invoke hooks here — defer hook invocation to caller to avoid duplicates

                Poll::Ready(Err(wrapped))
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Extension trait adding `.and_then_async()` for chaining futures returning results.
///
/// This allows chaining asynchronous computations that depend on the success of the previous one.
pub trait AsyncResultChainExt<T, E>: Future<Output = Result<T, E>> + Sized {
    /// Chains an asynchronous computation to execute if the previous future resolves to `Ok`.
    ///
    /// The closure `f` takes the successful value and returns a new future producing a `Result`.
    ///
    /// # Parameters
    /// - `f`: the chaining closure producing the next future.
    ///
    /// # Returns
    /// A future that resolves to the chained computation’s `Result`.
    fn and_then_async<Fut, F, U>(self, f: F) -> AndThenAsync<Self, Fut, F>
    where
        F: FnOnce(T) -> Fut,
        Fut: Future<Output = Result<U, E>>,
    {
        AndThenAsync {
            state: AndThenAsyncState::First(self, Some(f)),
        }
    }
}

impl<T, E, F> AsyncResultChainExt<T, E> for F where F: Future<Output = Result<T, E>> + Sized {}

/// Internal enum representing the current state of the chained async future.
pub enum AndThenAsyncState<Fut1, Fut2, F> {
    First(Fut1, Option<F>),
    Second(Fut2),
    Done,
}

/// Future that chains two async computations sequentially.
///
/// Internally manages polling of the first, then the second future produced by the chaining closure.
pub struct AndThenAsync<Fut1, Fut2, F> {
    state: AndThenAsyncState<Fut1, Fut2, F>,
}

impl<Fut1, Fut2, F, T, U, E> Future for AndThenAsync<Fut1, Fut2, F>
where
    Fut1: Future<Output = Result<T, E>>,
    Fut2: Future<Output = Result<U, E>>,
    F: FnOnce(T) -> Fut2,
{
    type Output = Result<U, E>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // Safety: Moving pinned fields in pattern matching is allowed here.
        let this = unsafe { self.get_unchecked_mut() };
        loop {
            match &mut this.state {
                AndThenAsyncState::First(fut1, maybe_f) => {
                    let fut1_pin = unsafe { Pin::new_unchecked(fut1) };
                    match fut1_pin.poll(cx) {
                        Poll::Pending => return Poll::Pending,
                        Poll::Ready(result) => match result {
                            Ok(value) => {
                                let f = maybe_f.take().expect("FnOnce already taken");
                                let fut2 = f(value);
                                this.state = AndThenAsyncState::Second(fut2);
                            }
                            Err(e) => {
                                this.state = AndThenAsyncState::Done;
                                return Poll::Ready(Err(e));
                            }
                        },
                    }
                }
                AndThenAsyncState::Second(fut2) => {
                    let fut2_pin = unsafe { Pin::new_unchecked(fut2) };
                    match fut2_pin.poll(cx) {
                        Poll::Pending => return Poll::Pending,
                        Poll::Ready(result) => {
                            this.state = AndThenAsyncState::Done;
                            return Poll::Ready(result);
                        }
                    }
                }
                AndThenAsyncState::Done => panic!("Polled after completion"),
            }
        }
    }
}
