use std::{
    ops::ControlFlow,
    time::{Duration, Instant},
};

use flume::{bounded, Receiver, RecvError, RecvTimeoutError, Selector, Sender};
use oneshot;

type WorkItem<C> = Box<dyn FnOnce(&mut C) + Send + 'static>;

pub(crate) struct WorkSender<C: ?Sized> {
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

impl<C: ?Sized> WorkSender<C> {
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

pub(crate) struct Worker<C: ?Sized> {
    handle: Receiver<WorkItem<C>>,
}

impl<C: ?Sized> Worker<C> {
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

    pub(crate) fn process_blocking<B: FnOnce() -> R + Send, R: Send>(
        &self,
        context: &mut C,
        blocking_op: B,
    ) -> (R, Option<RecvError>) {
        let (done_s, done_r) = bounded::<()>(1);

        let mut block_res = None;
        let mut recv_error = Ok(());

        rayon::in_place_scope(|s| {
            s.spawn(|_| {
                block_res = Some(blocking_op());
                done_s.send(()).unwrap();
            });

            recv_error = loop {
                let res = Selector::new()
                    .recv(&self.handle, |work| match work.map(|work| work(context)) {
                        Ok(_) => ControlFlow::Continue(()),
                        Err(e) => ControlFlow::Break(Err(e)),
                    })
                    .recv(&done_r, ControlFlow::Break)
                    .wait();

                if let ControlFlow::Break(e) = res {
                    break e;
                }
            };
        });
        (block_res.unwrap(), recv_error.err())
    }
}

pub(crate) fn workqueue<C: ?Sized>(size: usize) -> (WorkSender<C>, Worker<C>) {
    let (tx, rx) = bounded(size);

    (WorkSender { handle: tx }, Worker { handle: rx })
}
