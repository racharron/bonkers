use crate::{when, RequestCollection};

impl crate::ThreadPool for threadpool::ThreadPool {
    fn run<T: FnOnce() + Send + Sync + 'static>(&self, task: T) {
        self.execute(task)
    }
}

impl crate::Runner for threadpool::ThreadPool {
    type ThreadPool = Self;

    fn when<RC, T>(&self, cowns: RC, thunk: T)
    where
        RC: RequestCollection,
        T: for<'a> FnOnce(RC::Locked<'a>) + Send + Sync + 'static,
    {
        when(self, cowns, thunk);
    }

    fn threadpool(&self) -> &Self::ThreadPool {
        self
    }
}
