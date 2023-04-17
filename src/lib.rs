#![allow(dead_code)]

mod linked_list;
mod waker;

use crate::waker::Waker;
use linked_list::{LinkedList, Node};
use portable_atomic::{AtomicUsize, Ordering};

pub use waker::{State, WaitGuard};

#[derive(Debug, Default)]
pub struct Event {
    chain: parking_lot::Mutex<LinkedList<Waker>>,
    num_listeners: AtomicUsize,
}

impl Event {
    pub fn listen(&self) -> WaitGuard {
        let (waker, guard) = Waker::new();
        let node = Node::new(waker);
        {
            let mut lock = self.chain.lock();
            self.num_listeners.fetch_add(1, Ordering::Release);
            lock.push_node_back(node);
        }
        guard
    }

    pub fn listen_async(&self, waker: core::task::Waker) -> WaitGuard {
        let (waker, guard) = Waker::new_async(waker);
        let node = Node::new(waker);
        {
            let mut lock = self.chain.lock();
            self.num_listeners.fetch_add(1, Ordering::Release);
            lock.push_node_back(node);
        }
        guard
    }

    pub fn notify_one(&self) {
        portable_atomic::fence(Ordering::SeqCst);
        if self.num_listeners.load(Ordering::Acquire) == 0 {
            return;
        }
        let old_node;
        {
            let mut lock = self.chain.lock();
            let mut count = 1;
            loop {
                if let Some(node) = lock.pop_front() {
                    if !node.is_dropped() {
                        old_node = node;
                        break;
                    }
                } else {
                    return;
                }
                count += 1;
            }
            self.num_listeners.fetch_sub(count, Ordering::Relaxed);
        }
        old_node.wake();
    }

    pub fn notify_all(&self) {
        portable_atomic::fence(Ordering::SeqCst);
        if self.num_listeners.load(Ordering::Acquire) == 0 {
            return;
        }
        let mut old_chain;
        {
            let mut lock = self.chain.lock();
            self.num_listeners.store(0, Ordering::Relaxed);
            old_chain = lock.take_list();
        }

        for node in old_chain.iter_mut() {
            unsafe { node.as_ref() }.wake();
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
                assert_eq!(guard.wait(), State::Notified);
                assert_eq!(value.load(Ordering::Acquire), 42);
            });
            thread::sleep(Duration::from_millis(50));
            value.store(42, Ordering::Release);
            event.notify_one();

            let _ = jh.join();
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
            assert_eq!(event.num_listeners.load(Ordering::Acquire), 3);
            event.notify_one();
            assert_eq!(event.num_listeners.load(Ordering::Acquire), 0);

            let _ = jh.join();
        })
    }
}
