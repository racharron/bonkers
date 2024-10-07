use crate::{when, RequestRefCollection, RequestCollectionSuper};

impl crate::ThreadPool for threadpool::ThreadPool {
    fn run<T: FnOnce() + Send + Sync + 'static>(&self, task: T) {
        self.execute(task)
    }
}

impl crate::Runner for threadpool::ThreadPool {
    type ThreadPool = Self;

    fn when<CC, T>(&self, cowns: CC, thunk: T)
    where
        CC: RequestRefCollection,
        T: for<'a> FnOnce(<CC::Owned as RequestCollectionSuper>::Locked<'a>) + Send + Sync + 'static,
    {
        when(self, cowns, thunk);
    }

    fn threadpool(&self) -> &Self::ThreadPool {
        self
    }
}
