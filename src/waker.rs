use portable_atomic::AtomicU8;
use std::sync::atomic::Ordering;
use std::sync::Arc;

#[repr(u8)]
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum State {
    Waiting = 0,
    Notified = 1,
    Dropped = 2,
}

impl From<u8> for State {
    fn from(value: u8) -> Self {
        match value {
            0 => State::Waiting,
            1 => State::Notified,
            2 => State::Dropped,
            _ => panic!("unknown state"),
        }
    }
}

#[derive(Debug)]
enum InnerWaker {
    Sync(std::thread::Thread),
    Async(core::task::Waker),
}

impl InnerWaker {
    fn wake(&self) {
        match self {
            InnerWaker::Sync(thread) => thread.unpark(),
            InnerWaker::Async(waker) => waker.wake_by_ref(),
        }
    }
}

#[derive(Debug)]
pub struct Waker {
    inner: InnerWaker,
    state: Arc<AtomicU8>,
}

impl Drop for Waker {
    fn drop(&mut self) {
        let _ = self.state.compare_exchange(
            State::Waiting as u8,
            State::Dropped as u8,
            Ordering::SeqCst,
            Ordering::Relaxed,
        );
    }
}

impl Waker {
    pub fn new() -> (Self, WaitGuard) {
        let waker = Self {
            inner: InnerWaker::Sync(std::thread::current()),
            state: Arc::new(AtomicU8::new(State::Waiting as u8)),
        };
        let sleeper = waker.guard();
        (waker, sleeper)
    }

    pub fn new_async(waker: core::task::Waker) -> (Self, WaitGuard) {
        let waker = Self {
            inner: InnerWaker::Async(waker),
            state: Arc::new(AtomicU8::new(State::Waiting as u8)),
        };
        let sleeper = waker.guard();
        (waker, sleeper)
    }

    pub fn wake(&self) {
        let state = self.state.compare_exchange(
            State::Waiting as u8,
            State::Notified as u8,
            Ordering::SeqCst,
            Ordering::Relaxed,
        );
        if state.is_ok() {
            self.inner.wake();
            return;
        }
        debug_assert_eq!(state.unwrap_err(), State::Dropped as u8);
    }

    fn reset(&self) {
        self.state.store(State::Waiting as u8, Ordering::SeqCst);
    }
    fn reset_async(&mut self, waker: core::task::Waker) {
        self.state.store(State::Waiting as u8, Ordering::SeqCst);
        self.inner = InnerWaker::Async(waker);
    }
    fn guard(&self) -> WaitGuard {
        WaitGuard::new(self.state.clone())
    }
}

#[derive(Debug)]
pub struct WaitGuard {
    state: Arc<AtomicU8>,
}

impl Drop for WaitGuard {
    fn drop(&mut self) {
        self.state.store(State::Dropped as u8, Ordering::Release);
    }
}

impl WaitGuard {
    pub fn new(state: Arc<AtomicU8>) -> Self {
        Self { state }
    }

    pub fn wait(&self) -> State {
        loop {
            let state = self.state.load(Ordering::Acquire).into();
            match &state {
                State::Waiting => {
                    std::thread::park();
                }
                _ => return state,
            }
        }
    }

    pub fn get_state(&self) -> State {
        self.state.load(Ordering::Acquire).into()
    }
}

#[cfg(test)]
mod waker_tests {
    use super::*;

    #[test]
    fn basic() {
        let (sender, recv) = std::sync::mpsc::channel();

        std::thread::scope(|s| {
            let jh = s.spawn(move || {
                let (waker_handle, sleeper) = Waker::new();
                sender.send(waker_handle).expect("send failed");
                let state = sleeper.wait();
                assert_eq!(state, State::Notified);
            });
            let waker = recv.recv().expect("recv failed");
            std::thread::sleep(std::time::Duration::from_millis(100));
            assert_eq!(
                State::from(waker.state.load(Ordering::Relaxed)),
                State::Waiting
            );
            waker.wake();
            assert_eq!(
                State::from(waker.state.load(Ordering::Relaxed)),
                State::Notified
            );
            let _ = jh.join();
        })
    }

    #[test]
    fn dropped() {
        let (sender, recv) = std::sync::mpsc::channel();

        std::thread::scope(|s| {
            let jh = s.spawn(move || {
                let (waker_handle, sleeper) = Waker::new();
                sender.send(waker_handle).expect("send failed");
                drop(sleeper);
            });
            let waker = recv.recv().expect("recv failed");
            std::thread::sleep(std::time::Duration::from_millis(100));
            waker.wake();
            assert_eq!(
                State::from(waker.state.load(Ordering::Relaxed)),
                State::Dropped
            );
            let _ = jh.join();
        })
    }
}
