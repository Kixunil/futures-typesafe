use ::Future;

/// Represents future that is unsafe to use.
/// The callers of `poll()` must make sure that they don't call it again after the future resolves.
///
/// Panicking from `poll()` method is considered resolving future (with error).
///
/// Since it's internal trait, it doesn't make sense to implement combinators.
trait UnsafeFuture {
    type Item;
    type Error;

    /// Implementors of this method must not invoke UB in any case except for calling this
    /// method on resolved future.
    /// Users of this method must make sure that the function is not called on same object
    /// again, after it resolves.
    /// 
    /// The semantics of this method are same as of ::futures::Future::poll(). The only difference
    /// is that calling this method on resolved future is UB, and calling
    /// ::futures::Future::poll() on resolved future is NOT UB.
    ///
    /// Panicking from this method is same as resolving the future. (It's UB to call this method
    /// after panic.)
    unsafe fn poll(&mut self) -> ::futures::Poll<Self::Item, Self::Error>;
}

/// Helper for type safe future.
///
/// The `Option` serves as a drop flag. It must be statically guaranteed that it is `Some` unless
/// the future is resolved.
struct TSFuture<F> (Option<F>);

impl<F> TSFuture<F> {
    /// Constructor for clarity
    fn new(f: F) -> Self {
        // This must initialize the TSFuture with `Some` to indicate unresolved future and store
        // it's data.
        TSFuture(Some(f))
    }
}

/// Helper for type unsafe future.
struct TUFuture<F> (F);

impl<F> TUFuture<F> {
    /// Constructor for clarity
    fn new(f: F) -> Self {
        TUFuture(f)
    }
}

impl<F: ::futures::Future> UnsafeFuture for TUFuture<F> {
    type Item = F::Item;
    type Error = F::Error;

    unsafe fn poll(&mut self) -> ::futures::Poll<Self::Item, Self::Error> {
        // This implementation is correct because it actually doesn't call any unsafe function.
        self.0.poll()
    }
}

impl<F: Future> UnsafeFuture for TSFuture<F> {
    type Item = F::Item;
    type Error = F::Error;

    unsafe fn poll(&mut self) -> ::futures::Poll<Self::Item, Self::Error> {
        // This implementation is correct because unresolved future is always Some(F) and resolved
        // is always None. This implementation calls two unsafe functions: unchecked_unwrap() and
        // unchecked_unwrap_none().
        // Since this method can be called only on unresolved future and that always contains
        // Some(F), then unchecked_unwrap() must be correct.
        //
        // unchecked_unwrap_none() is called only after the `Option` was set to `None` via `take()`
        // method. There is no way it could've been modified meanwhile.
        //
        // unchecked_unwrap_none() may be not needed because compiler could be smart enough to see
        // that but redundant information should be OK and may even help compiler.
        use ::unreachable::UncheckedOptionExt;
        use ::std::mem::replace;

        // Replace whatever was in option with `None` and then call `unchecked_unwrap()` on
        // returned `Option` because unresolved future is always Some(F) and this method can be
        // called only on unresolved future. Finally, call `poll()` on the result.
        //
        // Note that using `Option` instead of simple `std::mem::read()` allows correct behaviour
        // in case of panic.
        let res = self.0.take().unchecked_unwrap().poll();
        match res {
            // Return appropriate result. Since the future is resolved, the `Option` stays `None`
            // to mark it as such.
            ::Poll::Success(item) => Ok(::futures::Async::Ready(item)),
            // Return appropriate result. Since the future is resolved, the `Option` stays `None`
            // to mark it as such.
            ::Poll::Failure(error) => Err(error),
            ::Poll::NotReady(future) => {
                // The future was temporarily marked as resolved to account for panics during
                // `poll()`. It obviously isn't resolved, so `Some` must be restored.
                // Absence of this line would obviously cause UB.
                replace(&mut self.0, Some(future)).unchecked_unwrap_none();
                // Return appropriate result.
                Ok(::futures::Async::NotReady)
            }
        }
    }
}

/// Boxes a `Future`
///
/// The purpose of this type is to erase underlying implementation using dynamic dispatch. It
/// allows making array (`Vec`) of type safe futures.
///
/// FutureObject always contains unresolved future. Once the future is resolved, it must cease to
/// exist.
pub struct FutureObject<I, E> (Box<UnsafeFuture<Item=I, Error=E>>);

impl<I, E> FutureObject<I, E> {
    /// Creates `FutureObject` using type safe implementation of future.
    pub fn from_type_safe_future<F: 'static + Future<Item=I, Error=E>>(future: F) -> Self {
        FutureObject(Box::new(TSFuture::new(future)))
    }

    /// Creates `FutureObject` using type unsafe implementation of future.
    pub fn from_type_unsafe_future<F: 'static + ::futures::Future<Item=I, Error=E>>(future: F) -> Self {
        FutureObject(Box::new(TUFuture::new(future)))
    }
}

impl<I, E> Future for FutureObject<I, E> {
    type Item = I;
    type Error = E;

    fn poll(mut self) -> ::Poll<Self> {
        // This code operates on `UnsafeFuture`, so it must make sure that `poll` isn't called
        // after future is resolved. Since it has safe interface, it has to make such call
        // statically impossible.
        unsafe {
            // The FutureObject must contain only unresolved future. Therefore calling poll() here is
            // correct.
            let res = self.0.poll();
            match res {
                // self is not kept here but dropped because the future is resolved now
                Ok(::futures::Async::Ready(item)) => ::Poll::Success(item),
                // Since the future is not resolved, self is returned to allow polling again
                Ok(::futures::Async::NotReady) => ::Poll::NotReady(self),
                // self is not kept here but dropped because the future is resolved now
                Err(error) => ::Poll::Failure(error),
            }
        }
    }
}
