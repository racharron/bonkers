mod simple;
mod os_mock;

pub use simple::SimpleThreadPool;
pub use os_mock::OsThreads;

pub trait ThreadPool {
    fn run<T: FnOnce() + Send + Sync + 'static>(&self, task: T);
    fn yield_here(&self);
}

