use std::{
    collections::HashMap,
    hash::Hash,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

use tokio::sync::Notify;

pub struct SingleFlight<K> {
    active: Arc<Mutex<HashMap<K, Arc<Flight>>>>,
}

impl<K> Default for SingleFlight<K> {
    fn default() -> Self {
        Self {
            active: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl<K: Clone + Eq + Hash> SingleFlight<K> {
    pub fn register(&self, key: K) -> Result<FlightLeader<K>, FlightWaiter> {
        let mut active = self
            .active
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(flight) = active.get(&key) {
            return Err(FlightWaiter {
                flight: flight.clone(),
            });
        }
        let flight = Arc::new(Flight::new());
        active.insert(key.clone(), flight.clone());
        Ok(FlightLeader {
            active: self.active.clone(),
            key,
            flight,
        })
    }
}

pub struct FlightWaiter {
    flight: Arc<Flight>,
}

impl FlightWaiter {
    pub async fn wait(self) {
        while !self.flight.complete.load(Ordering::Acquire) {
            let changed = self.flight.changed.notified();
            if self.flight.complete.load(Ordering::Acquire) {
                return;
            }
            changed.await;
        }
    }
}

pub struct FlightLeader<K: Eq + Hash> {
    active: Arc<Mutex<HashMap<K, Arc<Flight>>>>,
    key: K,
    flight: Arc<Flight>,
}

impl<K: Eq + Hash> Drop for FlightLeader<K> {
    fn drop(&mut self) {
        let mut active = self
            .active
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if active
            .get(&self.key)
            .is_some_and(|current| Arc::ptr_eq(current, &self.flight))
        {
            active.remove(&self.key);
        }
        self.flight.complete.store(true, Ordering::Release);
        self.flight.changed.notify_waiters();
    }
}

struct Flight {
    complete: AtomicBool,
    changed: Notify,
}

impl Flight {
    fn new() -> Self {
        Self {
            complete: AtomicBool::new(false),
            changed: Notify::new(),
        }
    }
}
