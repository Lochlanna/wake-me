use portable_atomic::AtomicU8;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;

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
    fn wake_by_ref(&self) {
        match self {
            InnerWaker::Sync(thread) => thread.unpark(),
            InnerWaker::Async(waker) => waker.wake_by_ref(),
        }
    }

    fn wake(self) {
        match self {
            InnerWaker::Sync(thread) => thread.unpark(),
            InnerWaker::Async(waker) => waker.wake(),
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
        self.inner.wake_by_ref();
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

    pub fn wake(&self) -> bool {
        let state = self.state.compare_exchange(
            State::Waiting as u8,
            State::Notified as u8,
            Ordering::SeqCst,
            Ordering::Relaxed,
        );
        if state.is_ok() {
            self.inner.wake_by_ref();
            return true;
        }
        debug_assert_eq!(state.unwrap_err(), State::Dropped as u8);
        false
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitError {
    Timeout,
}

impl core::fmt::Display for WaitError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            WaitError::Timeout => write!(f, "timeout"),
        }
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

    pub fn wait(&self) {
        loop {
            match self.get_state() {
                State::Waiting => {
                    std::thread::park();
                }
                _ => return,
            }
        }
    }

    pub fn wait_deadline(&self, deadline: Instant) -> Result<(), WaitError> {
        let mut max_park_duration = Instant::now().saturating_duration_since(deadline);
        while !max_park_duration.is_zero() {
            match self.get_state() {
                State::Waiting => {
                    std::thread::park_timeout(max_park_duration);
                    max_park_duration = deadline.saturating_duration_since(Instant::now());
                }
                _ => return Ok(()),
            }
        }
        Err(WaitError::Timeout)
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
                sleeper.wait();
                assert_eq!(
                    State::from(sleeper.state.load(Ordering::Acquire)),
                    State::Notified
                );
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
            jh.join().expect("join failed");
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
            jh.join().expect("join failed");
        })
    }
}
