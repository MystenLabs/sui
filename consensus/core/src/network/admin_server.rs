// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use axum::{extract::Extension, http::StatusCode, routing::get, Router};
use futures::channel::oneshot;
use mysten_metrics::spawn_logged_monitored_task;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::time::sleep;
use tracing::{error, info};

use crate::core_thread::{ChannelCoreThreadDispatcher, CoreThreadDispatcher};

pub(crate) struct AdminServerHandle {
    join_handle: tokio::task::JoinHandle<()>,
    stop: oneshot::Sender<()>,
}

impl AdminServerHandle {
    pub async fn stop(self) {
        // Abort the task and wait for it to finish
        self.stop.send(()).ok();
        self.join_handle.await.ok();
    }
}

pub fn start_admin_server(core_dispatcher: Arc<ChannelCoreThreadDispatcher>) -> AdminServerHandle {
    const PORT: u16 = 8085;

    let mut router = Router::new()
        .route("/enable_proposal_checks", get(enable_proposal_checks))
        .route("/disable_proposal_checks", get(disable_proposal_checks));

    router = router.layer(Extension(core_dispatcher));

    let socket_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), PORT);
    info!(
        address =% socket_address,
        "starting admin server"
    );

    let (tx_stop, rx_stop) = oneshot::channel();

    let handle = spawn_logged_monitored_task!(
        async move {
            // retry a few times before quitting
            let mut total_retries = 10;

            loop {
                total_retries -= 1;

                match TcpListener::bind(socket_address).await {
                    Ok(listener) => {
                        axum::serve(listener, router)
                            .with_graceful_shutdown(async move {
                                rx_stop.await.ok();
                            })
                            .await
                            .unwrap_or_else(|err| {
                                panic!("Failed to boot admin {}: {err}", socket_address)
                            });

                        return;
                    }
                    Err(err) => {
                        if total_retries == 0 {
                            error!("{}", err);
                            panic!("Failed to boot admin {}: {}", socket_address, err);
                        }

                        error!("{}", err);

                        // just sleep for a bit before retrying in case the port
                        // has not been de-allocated
                        sleep(Duration::from_secs(1)).await;

                        continue;
                    }
                }
            }
        },
        "AdminServerTask"
    );

    AdminServerHandle {
        join_handle: handle,
        stop: tx_stop,
    }
}

async fn enable_proposal_checks(
    Extension(core_dispatcher): Extension<Arc<ChannelCoreThreadDispatcher>>,
) -> StatusCode {
    core_dispatcher
        .ignore_proposal_checks_for_testing(false)
        .ok();
    StatusCode::OK
}

async fn disable_proposal_checks(
    Extension(core_dispatcher): Extension<Arc<ChannelCoreThreadDispatcher>>,
) -> StatusCode {
    core_dispatcher
        .ignore_proposal_checks_for_testing(true)
        .ok();
    StatusCode::OK
}
