use std::time::{Duration, Instant};

use crossbeam::channel::{bounded, Receiver, RecvTimeoutError, Sender, TryRecvError};
use oneshot;

type WorkItem<C> = Box<dyn FnOnce(&mut C) + Send + 'static>;

pub(crate) struct WorkSender<C> {
    handle: Sender<WorkItem<C>>,
}

// derive gets the bounds wrong
impl<C> Clone for WorkSender<C> {
    fn clone(&self) -> Self {
        WorkSender {
            handle: self.handle.clone(),
        }
    }
}

#[derive(Debug)]
pub(crate) enum Error {
    SendError,
    WorkDropped,
}

impl<C> WorkSender<C> {
    pub(crate) fn schedule_raw<F: FnOnce(&mut C) -> T + Send + 'static, T: Send + 'static>(
        &self,
        f: F,
    ) -> Result<oneshot::Receiver<T>, Error> {
        let (sender, receiver) = oneshot::channel();

        let work = |c: &mut C| {
            sender.send(f(c)).expect("receiver dissapeared");
        };

        self.handle
            .send(Box::new(work))
            .map_err(|_| Error::SendError)?;

        Ok(receiver)
    }

    pub(crate) fn schedule_wait<F: FnOnce(&mut C) -> T + Send + 'static, T: Send + 'static>(
        &self,
        f: F,
    ) -> Result<T, Error> {
        self.schedule_raw(f)?.recv().map_err(|_| Error::WorkDropped)
    }

    pub(crate) fn schedule_nowait<F: FnOnce(&mut C) + Send + 'static>(
        &self,
        f: F,
    ) -> Result<(), Error> {
        self.handle
            .send(Box::new(f))
            .map_err(|_| Error::SendError)?;

        Ok(())
    }
}

pub(crate) struct Worker<C> {
    handle: Receiver<WorkItem<C>>,
}

impl<C> Worker<C> {
    #[allow(unused)]
    pub(crate) fn process_pending(&self, context: &mut C) -> Result<(), TryRecvError> {
        loop {
            match self.handle.try_recv() {
                Ok(work) => work(context),
                Err(TryRecvError::Empty) => break Ok(()),
                Err(e) => break Err(e),
            }
        }
    }

    pub(crate) fn process_until(
        &self,
        context: &mut C,
        deadline: Instant,
    ) -> Result<(), RecvTimeoutError> {
        loop {
            match self.handle.recv_deadline(deadline) {
                Ok(work) => work(context),
                Err(RecvTimeoutError::Timeout) => break Ok(()),
                Err(e) => break Err(e),
            }
        }
    }

    pub(crate) fn process_sleep(
        &self,
        context: &mut C,
        sleep: Duration,
    ) -> Result<(), RecvTimeoutError> {
        let deadline = std::time::Instant::now() + sleep;
        self.process_until(context, deadline)
    }
}

pub(crate) fn workqueue<C>(size: usize) -> (WorkSender<C>, Worker<C>) {
    let (tx, rx) = bounded(size);

    (WorkSender { handle: tx }, Worker { handle: rx })
}
