// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::*;

use parking_lot::Mutex;
use std::collections::HashMap;
use tokio::sync::mpsc::UnboundedSender;

pub struct AutoIncSenderInner<T> {
    pub next_available_sequence_number: u64,
    pub next_expected_sequence_number: u64,
    pub sender: UnboundedSender<(u64, T)>,
    pub waiting: HashMap<u64, Option<T>>,
}

impl<T> AutoIncSenderInner<T> {
    pub fn send_all_waiting(&mut self) {
        while let Some(item_opt) = self.waiting.remove(&self.next_expected_sequence_number) {
            if let Some(item) = item_opt {
                let _ = self
                    .sender
                    .send((self.next_expected_sequence_number, item));

                /*
                    An error here indicates the other side of the channel is closed.
                    There is not very much we can do, as if the batcher is closed we
                    will write to the DB and the recover when we recover.
                */
            }
            self.next_expected_sequence_number += 1;
        }
    }
}

/*
    A wrapper around a channel sender that ensures items sent are associated with
    integer tickets and sent in increasing ticket order. When a ticket is dropped
    its ticket value is skipped and the subsequent tickets are sent.
*/

#[derive(Clone)]
pub struct AutoIncSender<T>(pub Arc<Mutex<AutoIncSenderInner<T>>>);

impl<T> AutoIncSender<T> {
    // Creates a new auto-incrementing sender
    pub fn new(sender: UnboundedSender<(u64, T)>, next_sequence_number: u64) -> AutoIncSender<T> {
        AutoIncSender(Arc::new(Mutex::new(AutoIncSenderInner {
            // TODO: next_available_sequence_number could be an AtomicU64 instead.
            next_available_sequence_number: next_sequence_number,
            next_expected_sequence_number: next_sequence_number,
            sender,
            waiting: HashMap::new(),
        })))
    }

    /// Creates a new ticket with the next available sequence number.
    pub fn next_ticket(&self) -> Ticket<T> {
        let ticket_number = {
            // Keep the critical region as small as possible
            let mut inc_sender = self.0.lock();
            let ticket_number_inner = inc_sender.next_available_sequence_number;
            inc_sender.next_available_sequence_number += 1;
            ticket_number_inner
        };

        Ticket {
            autoinc_sender: self.0.clone(),
            sequence_number: ticket_number,
            sent: false,
        }
    }
}

/// A ticket represents a slot in the sequence to be sent in the channel
pub struct Ticket<T> {
    autoinc_sender: Arc<Mutex<AutoIncSenderInner<T>>>,
    sequence_number: u64,
    sent: bool,
}

impl<T> Ticket<T>
where
    T: std::fmt::Debug,
{
    /// Send an item at that sequence in the channel.
    pub async fn send(&mut self, item: T) {
        let mut aic = self.autoinc_sender.lock();
        aic.waiting.insert(self.sequence_number, Some(item));
        println!("SEND {:?}", aic.waiting);
        self.sent = true;
        aic.send_all_waiting();
    }

    /// Get the ticket sequence number
    pub fn ticket(&self) -> u64 {
        self.sequence_number
    }
}

/// A custom drop that indicates that there may not be a item
/// associated with this sequence number,
impl<T> Drop for Ticket<T> {
    fn drop(&mut self) {
        if !self.sent {
            let mut aic = self.autoinc_sender.lock();
            aic.waiting.insert(self.sequence_number, None);
            aic.send_all_waiting();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_ticketing() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let autoinc = AutoIncSender::new(tx, 10);

        let mut t1 = autoinc.next_ticket();
        let t2 = autoinc.next_ticket();
        let t3 = autoinc.next_ticket();
        let mut t4 = autoinc.next_ticket();

        // Send a value out of order
        t4.send(1010).await;

        // Drop a ticket
        drop(t2);

        // Panic and lose a ticket in a task
        let handle = tokio::spawn(async move {
            let _inner = t3;
            panic!("Crash here!");
            // t3.send(1010).await;
        });

        // drive the task to completion, ie panic
        assert!(handle.await.is_err());

        // Send the initial ticket
        t1.send(1040).await;

        // Try to read
        let (s1, v1) = rx.recv().await.unwrap();
        let (s2, v2) = rx.recv().await.unwrap();

        assert_eq!(10, s1);
        assert_eq!(13, s2);
        assert_eq!(1040, v1);
        assert_eq!(1010, v2);
    }
}
