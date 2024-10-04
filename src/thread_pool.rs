mod os_mock;
#[cfg(feature = "rayon")]
mod rayon;
mod simple;
#[cfg(feature = "threadpool")]
mod threadpool;

pub use os_mock::OsThreads;
pub use simple::SimpleThreadPool;

/// Used to connect the behavior oriented concurrency primitives to threadpools.
pub trait ThreadPool {
    /// Queue a thunk to run on this threadpool.
    fn run<T: FnOnce() + Send + Sync + 'static>(&self, task: T);
}
