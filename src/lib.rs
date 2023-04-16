#![allow(dead_code)]

mod linked_list;
mod waker;

use crate::waker::Waker;
use linked_list::{LinkedList, Node};

pub use waker::{State, WaitGuard};

#[derive(Debug, Default)]
pub struct Event {
    chain: parking_lot::Mutex<LinkedList<Waker>>,
}

impl Event {
    pub fn listen(&self) -> WaitGuard {
        let (waker, guard) = Waker::new();
        let node = Node::new(waker);
        self.chain.lock().push_node_back(node);
        guard
    }

    pub fn listen_async(&self, waker: core::task::Waker) -> WaitGuard {
        let (waker, guard) = Waker::new_async(waker);
        let node = Node::new(waker);
        self.chain.lock().push_node_back(node);
        guard
    }

    pub fn notify_one(&self) {
        let old_node = self.chain.lock().pop_front();
        if let Some(old_node) = old_node {
            old_node.wake();
        }
    }

    pub fn notify_all(&self) {
        let mut old_chain = self.chain.lock().take_list();
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
}
