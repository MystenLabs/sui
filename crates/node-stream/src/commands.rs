// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::SocketAddr;

use clap::Parser;

#[derive(Parser)]
#[clap(rename_all = "kebab-case", author, version)]
pub enum NodeStreamCommand {
    #[clap(name = "listen")]
    Listen {
        #[clap(long)]
        producer: SocketAddr,

        #[clap(long)]
        id: String,
    },
}
impl NodeStreamCommand {
    pub async fn execute(self) -> Result<(), String> {
        match self {
            Self::Listen { producer, id } => {
                let mut consumer = crate::consumer::NodeStreamConsumer::new_with_id(
                    producer.to_string(),
                    id,
                    crate::types::NodeStreamTopic::ObjectChangeLight,
                    0,
                );
                loop {
                    let data = consumer.poll();
                    println!("Received: {:#?}", data);
                }
            }
        }
    }
}
