//! TODO Two cases to prevent internal optimizations from triggering (if they exist).

use std::ops::Deref;
use std::time::{Duration, Instant};

use channel;

#[derive(Clone)]
pub struct Sender<T>(pub channel::Sender<T>);

#[derive(Clone)]
pub struct Receiver<T>(pub channel::Receiver<T>);

impl<T> Deref for Receiver<T> {
    type Target = channel::Receiver<T>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> Deref for Sender<T> {
    type Target = channel::Sender<T>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> Sender<T> {
    pub fn send(&self, msg: T) {
        select! {
            send(self.0, msg) => {}
            send(self.0, msg) => {}
        }
    }
}

impl<T> Receiver<T> {
    pub fn try_recv(&self) -> Option<T> {
        select! {
            recv(self.0, msg) => msg,
            recv(self.0, msg) => msg,
            default => None,
        }
    }

    pub fn recv(&self) -> Option<T> {
        select! {
            recv(self.0, msg) => msg,
            recv(self.0, msg) => msg,
        }
    }
}

#[allow(dead_code)]
pub fn bounded<T>(cap: usize) -> (Sender<T>, Receiver<T>) {
    let (s, r) = channel::bounded(cap);
    (Sender(s), Receiver(r))
}

#[allow(dead_code)]
pub fn unbounded<T>() -> (Sender<T>, Receiver<T>) {
    let (s, r) = channel::unbounded();
    (Sender(s), Receiver(r))
}

pub fn after(dur: Duration) -> Receiver<Instant> {
    let r = channel::after(dur);
    Receiver(r)
}
