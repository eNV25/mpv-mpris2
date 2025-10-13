use mpris_server::Time;

#[inline]
pub(crate) fn time_as_secs(time: Time) -> f64 {
    time.as_micros() as f64 / 1_000_000.0
}

#[inline]
pub(crate) fn time_from_secs(secs: f64) -> Time {
    Time::from_micros((secs * 1_000_000.0) as i64)
}

pub(crate) trait SyncFutureExt: Sized + Future {
    fn sync(self) -> impl Future<Output = Self::Output> + Sync {
        // use std::sync::SyncView when stable
        sync_wrapper::SyncFuture::new(self)
    }
}

impl<F: Future> SyncFutureExt for F {}
