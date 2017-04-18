extern crate futures;
extern crate unreachable;

pub mod future_object;

pub enum Poll<F: Future> {
    Success(F::Item),
    Failure(F::Error),
    NotReady(F),
}

pub trait Future: Sized {
    type Item;
    type Error;

    fn poll(self) -> Poll<Self>;

    fn glue(self) -> Glue<Self> {
        Glue::Valid(self)
    }
}

/// This implements futures::Future with panicking if `poll()` is called on resolved future.
pub enum Glue<F> {
    Valid(F),
    Invalid,
}

impl<F: Future> futures::Future for Glue<F> {
    type Item = F::Item;
    type Error = F::Error;

    fn poll(&mut self) -> futures::Poll<Self::Item, Self::Error> {
        use ::std::mem::replace;

        let fut = match replace(self, Glue::Invalid) {
            Glue::Valid(fut) => fut,
            Glue::Invalid => panic!("Poll called on resolved Future!"),
        };

        match fut.poll() {
            Poll::Success(val) => Ok(futures::Async::Ready(val)),
            Poll::Failure(e) => Err(e),
            Poll::NotReady(f) => {
                *self = Glue::Valid(f);
                Ok(futures::Async::NotReady)
            }
        }
    }
}
