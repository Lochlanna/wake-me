#![allow(dead_code)]

mod waker;
use concurrent_queue::ConcurrentQueue;

use crate::waker::Waker;
use portable_atomic::Ordering;

pub use waker::{State, WaitGuard};

#[derive(Debug)]
pub struct Event {
    chain: ConcurrentQueue<Waker>,
}

impl Default for Event {
    fn default() -> Self {
        Self {
            chain: ConcurrentQueue::unbounded(),
        }
    }
}

impl Event {
    pub fn listen(&self) -> WaitGuard {
        let (waker, guard) = Waker::new();
        self.chain.push(waker).expect("couldn't push to queue");
        guard
    }

    pub fn listen_async(&self, waker: core::task::Waker) -> WaitGuard {
        let (waker, guard) = Waker::new_async(waker);
        self.chain.push(waker).expect("couldn't push to queue");
        guard
    }

    pub fn notify_one(&self) {
        portable_atomic::fence(Ordering::SeqCst);
        if self.chain.is_empty() {
            return;
        }
        while let Ok(node) = self.chain.pop() {
            if node.wake() {
                return;
            }
        }
    }

    // Can we add a take function to the queue to optimise this? / Would that actually be better?
    pub fn notify_all(&self) {
        portable_atomic::fence(Ordering::SeqCst);
        for _ in 0..self.chain.len() {
            if let Ok(node) = self.chain.pop() {
                node.wake();
            } else {
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::waker::State;
    use portable_atomic::{AtomicUsize, Ordering};
    use std::thread;
    use std::time::Duration;

    #[test]
    fn it_works() {
        let event = Event::default();
        let value = AtomicUsize::new(0);

        thread::scope(|s| {
            let jh = s.spawn(|| {
                let guard = event.listen();
                assert_eq!(value.load(Ordering::Acquire), 0);
                assert_eq!(guard.get_state(), State::Waiting);
                guard.wait();
                assert_eq!(value.load(Ordering::Acquire), 42);
                assert_eq!(guard.get_state(), State::Notified);
            });
            thread::sleep(Duration::from_millis(50));
            value.store(42, Ordering::Release);
            event.notify_one();

            jh.join().expect("couldn't join!");
        })
    }
    #[test]
    fn drop_count() {
        let event = Event::default();
        let value = AtomicUsize::new(0);

        thread::scope(|s| {
            let jh = s.spawn(|| {
                {
                    let _guard_a = event.listen();
                    let _guard_b = event.listen();
                }
                let guard_c = event.listen();
                guard_c.wait();
                assert_eq!(value.load(Ordering::Acquire), 42);
            });
            thread::sleep(Duration::from_millis(50));
            value.store(42, Ordering::Release);
            assert_eq!(event.chain.len(), 3);
            event.notify_one();
            assert_eq!(event.chain.len(), 0);

            jh.join().expect("couldn't join!");
        })
    }
}
