mod simple;
mod os_mock;

pub use simple::SimpleThreadPool;
pub use os_mock::OsThreads;

/// Used to connect the behavior oriented concurrency primitives to threadpools.
pub trait ThreadPool {
    /// Queue a thunk to run on this threadpool.
    fn run<T: FnOnce() + Send + Sync + 'static>(&self, task: T);
}

