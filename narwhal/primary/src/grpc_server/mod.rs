// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use std::time::Duration;

use self::validator::NarwhalValidator;
use crate::BlockCommand;
use tokio::sync::mpsc::Sender;
use tonic::transport::Server;
use tracing::error;
use types::ValidatorServer;

mod validator;

pub struct ConsensusAPIGrpc {
    socket_addr: String,
    tx_get_block_commands: Sender<BlockCommand>,
    get_collections_timeout: Duration,
}

impl ConsensusAPIGrpc {
    pub fn spawn(
        socket_addr: String,
        tx_get_block_commands: Sender<BlockCommand>,
        get_collections_timeout: Duration,
    ) {
        tokio::spawn(async move {
            let _ = Self {
                socket_addr,
                tx_get_block_commands,
                get_collections_timeout,
            }
            .run()
            .await
            .map_err(|e| error!("{:?}", e));
        });
    }

    async fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let addr = self.socket_addr.parse()?;
        let narwhal = NarwhalValidator::new(
            self.tx_get_block_commands.to_owned(),
            self.get_collections_timeout,
        );

        Server::builder()
            .add_service(ValidatorServer::new(narwhal))
            .serve(addr)
            .await?;

        Ok(())
    }
}
