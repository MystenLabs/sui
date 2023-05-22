// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{net::SocketAddr, str::FromStr};

use clap::Parser;

use crate::{
    consumer_ex::NodeStreamConsumer, example::TxInfoNodeStreamTopic, types_ex::NodeStreamSessionId,
};

#[derive(Parser)]
#[clap(rename_all = "kebab-case", author, version)]
pub enum NodeStreamCommand {
    #[clap(name = "listen")]
    Listen {
        #[clap(long, short)]
        producer_url: SocketAddr,

        #[clap(long, short)]
        session_id: Option<String>,

        #[clap(long, short)]
        topic: String,

        #[clap(long, short)]
        epoch: u64,
    },
}
impl NodeStreamCommand {
    pub async fn execute(self) -> Result<(), String> {
        match self {
            Self::Listen {
                producer_url,
                session_id,
                topic,
                epoch,
            } => {
                let session_id = session_id.map(|q| NodeStreamSessionId::from_str(&q).unwrap());
                let tp = TxInfoNodeStreamTopic::from_str(&topic).unwrap();
                let mut consumer =
                    NodeStreamConsumer::new(producer_url, session_id, epoch, tp).unwrap();
                println!("Listening on {}: session id: {}", producer_url, consumer.session_id());
                loop {
                    let data = consumer.poll().unwrap();
                    for d in data {
                        println!("Received: {:#?}", d);
                    }
                }
            }
        }
    }
}
