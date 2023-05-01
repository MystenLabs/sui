// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use futures::FutureExt;
use std::sync::{Arc, Weak};
use std::thread;
use sui_config::NodeConfig;
use sui_node::{metrics, SuiNode, SuiNodeHandle};
use sui_types::crypto::{AuthorityPublicKeyBytes, KeypairTraits};
use tracing::{info, trace};

use super::node::RuntimeType;

#[derive(Debug)]
pub(crate) struct Container {
    join_handle: Option<thread::JoinHandle<()>>,
    cancel_sender: Option<tokio::sync::oneshot::Sender<()>>,
    node: Weak<SuiNode>,
}

/// When dropped, stop and wait for the node running in this Container to completely shutdown.
impl Drop for Container {
    fn drop(&mut self) {
        trace!("dropping Container");

        let thread = self.join_handle.take().unwrap();

        let cancel_handle = self.cancel_sender.take().unwrap();

        // Notify the thread to shutdown
        let _ = cancel_handle.send(());

        // Wait for the thread to join
        thread.join().unwrap();

        trace!("finished dropping Container");
    }
}

impl Container {
    /// Spawn a new Node.
    pub async fn spawn(config: NodeConfig, runtime: RuntimeType) -> Self {
        let (startup_sender, startup_receiver) = tokio::sync::oneshot::channel();
        let (cancel_sender, cancel_receiver) = tokio::sync::oneshot::channel();

        let thread = thread::spawn(move || {
            let span = tracing::span!(
                tracing::Level::INFO,
                "node",
                name =% AuthorityPublicKeyBytes::from(config.protocol_key_pair().public()).concise(),
            );
            let _guard = span.enter();

            let mut builder = match runtime {
                RuntimeType::SingleThreaded => tokio::runtime::Builder::new_current_thread(),
                RuntimeType::MultiThreaded => {
                    thread_local! {
                        static SPAN: std::cell::RefCell<Option<tracing::span::EnteredSpan>> =
                            std::cell::RefCell::new(None);
                    }
                    let mut builder = tokio::runtime::Builder::new_multi_thread();
                    let span = span.clone();
                    builder
                        .on_thread_start(move || {
                            SPAN.with(|maybe_entered_span| {
                                *maybe_entered_span.borrow_mut() = Some(span.clone().entered());
                            });
                        })
                        .on_thread_stop(|| {
                            SPAN.with(|maybe_entered_span| {
                                maybe_entered_span.borrow_mut().take();
                            });
                        });

                    builder
                }
            };
            let runtime = builder.enable_all().build().unwrap();

            runtime.block_on(async move {
                let registry_service = metrics::start_prometheus_server(config.metrics_address);
                info!(
                    "Started Prometheus HTTP endpoint. To query metrics use\n\tcurl -s http://{}/metrics",
                    config.metrics_address
                );
                let server = SuiNode::start(&config, registry_service, None).await.unwrap();
                // Notify that we've successfully started the node
                let _ = startup_sender.send(Arc::downgrade(&server));
                // run until canceled
                cancel_receiver.map(|_| ()).await;

                trace!("cancellation received; shutting down thread");
            });
        });

        let node = startup_receiver.await.unwrap();

        Self {
            join_handle: Some(thread),
            cancel_sender: Some(cancel_sender),
            node,
        }
    }

    /// Get a SuiNodeHandle to the node owned by the container.
    pub fn get_node_handle(&self) -> Option<SuiNodeHandle> {
        Some(SuiNodeHandle::new(self.node.upgrade()?))
    }

    /// Check to see that the Node is still alive by checking if the receiving side of the
    /// `cancel_sender` has been dropped.
    ///
    //TODO When we move to rust 1.61 we should also use
    // https://doc.rust-lang.org/stable/std/thread/struct.JoinHandle.html#method.is_finished
    // in order to check if the thread has finished.
    pub fn is_alive(&self) -> bool {
        if let Some(cancel_sender) = &self.cancel_sender {
            !cancel_sender.is_closed()
        } else {
            false
        }
    }
}
