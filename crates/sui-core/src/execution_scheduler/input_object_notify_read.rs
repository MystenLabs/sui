// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Arc,
    },
    task::{Context, Poll},
};

use dashmap::DashMap;
use either::Either;
use itertools::Itertools;
use parking_lot::Mutex;
use sui_types::storage::InputKey;
use tokio::sync::{mpsc::UnboundedSender, oneshot};

use super::PendingCertificate;

pub(crate) struct InputObjectNotifyRead {
    index: AtomicU64,
    pending: DashMap<InputKey, HashMap<u64, Arc<Waiter>>>,
    tx_ready_certificates: UnboundedSender<PendingCertificate>,
}

struct WaiterData {
    certificate: PendingCertificate,
    finish_sender: oneshot::Sender<()>,
}

struct Waiter {
    index: u64,
    num_missing_keys: AtomicUsize,
    data: Mutex<Option<WaiterData>>,
}

struct Registration<'a> {
    this: &'a InputObjectNotifyRead,
    waiter: Arc<Waiter>,
    finish_receiver: oneshot::Receiver<()>,
    missing_keys: Vec<&'a InputKey>,
}

impl InputObjectNotifyRead {
    pub fn new(tx_ready_certificates: UnboundedSender<PendingCertificate>) -> Self {
        Self {
            index: AtomicU64::new(0),
            pending: DashMap::new(),
            tx_ready_certificates,
        }
    }

    pub fn notify(&self, key: &InputKey) {
        if let Some((_, waiters)) = self.pending.remove(key) {
            for (_, waiter) in waiters {
                waiter.notify(1, &self.tx_ready_certificates);
            }
        }
    }

    pub async fn schedule(
        &self,
        keys: &[InputKey],
        check_exists: impl Fn(&[InputKey]) -> Vec<bool>,
        certificate: PendingCertificate,
    ) {
        if keys.is_empty() {
            return;
        }
        let mut registration = self.register(keys, certificate);
        let exists = check_exists(keys);
        let (available_keys, missing_keys): (Vec<_>, Vec<_>) =
            keys.iter().zip(exists).partition_map(|(key, exists)| {
                if exists {
                    Either::Left(key)
                } else {
                    Either::Right(key)
                }
            });
        registration.missing_keys = missing_keys;
        let num_available_keys = available_keys.len();
        self.deregister(available_keys.into_iter(), registration.waiter.index);
        registration
            .waiter
            .notify(num_available_keys, &self.tx_ready_certificates);
        if num_available_keys != keys.len() {
            registration.await
        }
    }

    fn register<'a>(
        &'a self,
        keys: &'a [InputKey],
        certificate: PendingCertificate,
    ) -> Registration<'a> {
        let index = self.index.fetch_add(1, Ordering::Relaxed);
        let (finish_sender, finish_receiver) = oneshot::channel();
        let waiter = Arc::new(Waiter::new(index, keys.len(), certificate, finish_sender));
        for key in keys {
            self.pending
                .entry(*key)
                .or_default()
                .insert(index, waiter.clone());
        }
        Registration {
            this: self,
            waiter,
            finish_receiver,
            missing_keys: vec![],
        }
    }

    fn deregister<'a>(&'a self, keys: impl IntoIterator<Item = &'a InputKey>, index: u64) {
        for key in keys {
            if let Some(mut waiters) = self.pending.get_mut(key) {
                waiters.remove(&index);
            }
            self.pending.remove_if(key, |_, waiters| waiters.is_empty());
        }
    }
}

impl Waiter {
    fn new(
        index: u64,
        num_missing_keys: usize,
        certificate: PendingCertificate,
        finish_sender: oneshot::Sender<()>,
    ) -> Self {
        Self {
            index,
            num_missing_keys: AtomicUsize::new(num_missing_keys),
            data: Mutex::new(Some(WaiterData {
                certificate,
                finish_sender,
            })),
        }
    }

    fn notify(
        &self,
        num_missing_keys: usize,
        tx_ready_certificates: &UnboundedSender<PendingCertificate>,
    ) {
        let n = self
            .num_missing_keys
            .fetch_sub(num_missing_keys, Ordering::Relaxed);
        if n == num_missing_keys {
            let data = self.data.lock().take().unwrap();
            tx_ready_certificates.send(data.certificate).ok();
            data.finish_sender.send(()).ok();
        }
    }
}

impl Future for Registration<'_> {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let poll = Pin::new(&mut self.finish_receiver).poll(cx);
        poll.map(|r| r.expect("Sender never drops when registration is pending"))
    }
}

impl Drop for Registration<'_> {
    fn drop(&mut self) {
        self.this
            .deregister(self.missing_keys.iter().copied(), self.waiter.index);
    }
}
