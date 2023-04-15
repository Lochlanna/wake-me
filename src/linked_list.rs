use std::ops::Deref;
use std::ptr::NonNull;

#[derive(Debug)]
pub struct Node<T> {
    value: T,
    next: Option<NonNull<Node<T>>>,
    previous: Option<NonNull<Node<T>>>,
}

unsafe impl<T> Send for Node<T> where T: Send {}

impl<T> Deref for Node<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T> Node<T> {
    pub fn new(value: T) -> Box<Self> {
        Box::new(Self {
            value,
            next: None,
            previous: None,
        })
    }
}

#[derive(Debug)]
pub struct LinkedList<T> {
    head: Option<NonNull<Node<T>>>,
    tail: Option<NonNull<Node<T>>>,
}

unsafe impl<T> Send for LinkedList<T> where T: Send {}

impl<T> Drop for LinkedList<T> {
    fn drop(&mut self) {
        let mut current = self.head;
        while let Some(mut node) = current {
            unsafe {
                current = node.as_mut().next;
                let _ = Box::from_raw(node.as_ptr());
            }
        }
    }
}

impl<T> Default for LinkedList<T> {
    fn default() -> Self {
        Self {
            head: None,
            tail: None,
        }
    }
}

impl<T> LinkedList<T> {
    pub fn push_node_front(&mut self, node: Box<Node<T>>) {
        let node = Box::into_raw(node);
        let mut node = unsafe { NonNull::new_unchecked(node) };
        if let Some(mut head) = self.head {
            unsafe {
                head.as_mut().previous = Some(node);
            }
        }
        unsafe {
            node.as_mut().next = self.head;
        }
        self.head = Some(node);
        if self.tail.is_none() {
            self.tail = Some(node);
        }
    }
    pub fn push_node_back(&mut self, node: Box<Node<T>>) {
        let node = Box::into_raw(node);
        let mut node = unsafe { NonNull::new_unchecked(node) };
        if let Some(mut tail) = self.tail {
            unsafe {
                tail.as_mut().next = Some(node);
            }
        }
        unsafe {
            node.as_mut().previous = self.tail;
        }
        self.tail = Some(node);
        if self.head.is_none() {
            self.head = Some(node);
        }
    }
    pub fn push_back(&mut self, value: T) {
        let node = Node::new(value);
        self.push_node_back(node);
    }
    pub fn push_front(&mut self, value: T) {
        let node = Node::new(value);
        self.push_node_front(node);
    }

    pub fn pop_front(&mut self) -> Option<Box<Node<T>>> {
        if let Some(mut head) = self.head {
            self.head = unsafe { head.as_mut().next };
            if let Some(mut head) = self.head {
                unsafe {
                    head.as_mut().previous = None;
                }
            }
            if self.head.is_none() {
                self.tail = None;
            }
            Some(unsafe { Box::from_raw(head.as_ptr()) })
        } else {
            None
        }
    }

    pub fn pop_back(&mut self) -> Option<Box<Node<T>>> {
        if let Some(mut tail) = self.tail {
            self.tail = unsafe { tail.as_mut().previous };
            if let Some(mut tail) = self.tail {
                unsafe {
                    tail.as_mut().next = None;
                }
            }
            if self.tail.is_none() {
                self.head = None;
            }
            Some(unsafe { Box::from_raw(tail.as_ptr()) })
        } else {
            None
        }
    }

    pub fn take_list(&mut self) -> Self {
        let taken = Self {
            head: self.head,
            tail: self.tail,
        };
        self.head = None;
        self.tail = None;
        taken
    }

    pub fn iter_mut(&mut self) -> Cursor<T> {
        Cursor::new(self)
    }
}

pub struct Cursor<'a, T> {
    current: Option<NonNull<Node<T>>>,
    _phantom: &'a mut LinkedList<T>,
}

impl<'a, T> Cursor<'a, T> {
    pub fn new(list: &'a mut LinkedList<T>) -> Self {
        Self {
            current: list.head,
            _phantom: list,
        }
    }
}

impl<'a, T> Iterator for Cursor<'a, T> {
    type Item = NonNull<Node<T>>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(current) = self.current {
            unsafe {
                self.current = current.as_ref().next;
            }
            Some(current)
        } else {
            None
        }
    }
}

//test module for linked_list
#[cfg(test)]
mod linked_list_tests {
    use super::*;

    #[test]
    fn test_linked_list() {
        let mut list = LinkedList::default();
        list.push_front(1);
        list.push_front(2);
        list.push_front(3);
        list.push_front(4);
        let mut values = vec![];
        for node in list.iter_mut() {
            values.push(unsafe { node.as_ref().value });
        }
        assert_eq!(values, vec![4, 3, 2, 1]);
    }
}
