//! Fan-out of state snapshots to every connected IPC client.

use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Mutex, MutexGuard};

use zugluft_ipc::{Event, ServiceState};

pub struct Hub {
    latest: Mutex<ServiceState>,
    clients: Mutex<Vec<Sender<Event>>>,
}

impl Hub {
    pub fn new() -> Self {
        Self {
            latest: Mutex::new(ServiceState::Detecting),
            clients: Mutex::new(Vec::new()),
        }
    }

    /// Stores the state and pushes it to all live clients; dead client
    /// channels are pruned as a side effect.
    pub fn publish(&self, state: ServiceState) {
        *self.latest() = state.clone();
        self.clients()
            .retain(|client| client.send(Event::State(state.clone())).is_ok());
    }

    /// Registers a client; the current state is already queued on the
    /// returned receiver.
    pub fn subscribe(&self) -> Receiver<Event> {
        let (tx, rx) = channel();
        let _ = tx.send(Event::State(self.latest().clone()));
        self.clients().push(tx);
        rx
    }

    fn latest(&self) -> MutexGuard<'_, ServiceState> {
        self.latest
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn clients(&self) -> MutexGuard<'_, Vec<Sender<Event>>> {
        self.clients
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}
