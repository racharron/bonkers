mod simple;
mod os_mock;

pub use simple::SimpleThreadPool;
pub use os_mock::OsThreads;

/// Used to connect the behavior oriented concurrency primitives to threadpools.
pub trait ThreadPool {
    /// Queue a thunk to run on this threadpool.
    fn run<T: FnOnce() + Send + Sync + 'static>(&self, task: T);
    /// Attempt to run a queued task from the threadpool on the currently running thread.  Returns
    /// whether a task was run.  So a `true` indicates that the current thread executed a task from
    /// the threadpool, and a false indicates that it did not.  Most of the time, when this method
    /// returns false, [`std::thread::yield_here`] should be called.
    fn yield_here(&self) -> bool;
}

