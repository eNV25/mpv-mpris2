use mpris_server::Time;
use std::{
    pin::Pin,
    task::{Context, Poll},
};

#[inline]
pub(crate) fn time_as_secs(time: Time) -> f64 {
    time.as_micros() as f64 / 1_000_000.0
}

#[inline]
pub(crate) fn time_from_secs(secs: f64) -> Time {
    Time::from_micros((secs * 1_000_000.0) as i64)
}

pub(crate) trait FutureSyncExt: Sized + Future {
    fn sync(self) -> impl Future<Output = Self::Output> + Sync {
        #[repr(transparent)]
        struct SyncView<F: Future>(F);
        unsafe impl<F: Future> Sync for SyncView<F> {}
        impl<F: Future> Future for SyncView<F> {
            type Output = F::Output;
            #[inline]
            fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                let future = unsafe { self.map_unchecked_mut(|view| &mut view.0) };
                future.poll(cx)
            }
        }
        // use std::sync::SyncView when stable
        SyncView(self)
    }
}
impl<F: Future> FutureSyncExt for F {}
