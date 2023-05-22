// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::types_ex::{
    NodeStreamPayload, NodeStreamPerEpochTopic, NodeStreamProducerError, NodeStreamTopic,
};
use kafka::producer::{Producer, Record};
use std::{net::SocketAddr, time::Duration};

const TOPIC_REFRESH_TRIALS: u32 = 100;
const TOPIC_REFRESH_SLEEP: Duration = Duration::from_millis(100);
const PAYLOAD_SIZE_LIMIT: u64 = 1_000_000; // 1MB

pub struct NodeStreamProducer {
    host_addr: SocketAddr,
    kafka_producer: Producer,
}
impl NodeStreamProducer {
    pub fn new(host_addr: SocketAddr) -> Result<Self, NodeStreamProducerError> {
        let kafka_producer = Producer::from_hosts(vec![host_addr.to_string()])
            .create()
            .map_err(|err| NodeStreamProducerError::UnableToCreateProducer { err })?;
        Ok(Self {
            host_addr,
            kafka_producer,
        })
    }

    pub fn send<
        T: NodeStreamPerEpochTopic<D, M> + std::fmt::Debug,
        D: std::fmt::Debug,
        M: std::fmt::Debug,
    >(
        &mut self,
        epoch: u64,
        topic: T,
        payload: &NodeStreamPayload<D, M>,
    ) -> Result<(), NodeStreamProducerError> {
        let topic_str = topic.topic_for_epoch(epoch).to_raw();
        let bytes = topic.payload_to_bytes(payload).map_err(|err| {
            NodeStreamProducerError::PayloadSerializeError {
                topic: NodeStreamTopic {
                    topic: topic_str.clone(),
                },
                err: format!("{:?}", err),
            }
        })?;
        if bytes.len() as u64 > PAYLOAD_SIZE_LIMIT {
            return Err(NodeStreamProducerError::PayloadTooLarge {
                limit: PAYLOAD_SIZE_LIMIT,
                size: bytes.len() as u64,
            });
        }
        let record = Record::from_value(&topic_str, bytes);

        let mut num_trials = TOPIC_REFRESH_TRIALS;
        // TODO:
        // Check that topic exists before sending

        // Make sure up to date
        self.kafka_producer
            .client_mut()
            .load_metadata_all()
            .map_err(|err| NodeStreamProducerError::UnableToLoadAllInternalMetatada { err })?;

        // This will only happen first time this topis is hit
        while !self.kafka_producer.client().topics().contains(&topic_str) && num_trials > 0 {
            // Add the topic
            self.kafka_producer
                .client_mut()
                .load_metadata(&[&topic_str])
                .map_err(
                    |err| NodeStreamProducerError::UnableToLoadTopicInternalMetatada {
                        topic: topic.topic_for_epoch(epoch),
                        err,
                    },
                )?;
            num_trials -= 1;
            std::thread::sleep(TOPIC_REFRESH_SLEEP);
            // try to initialize again
            self.kafka_producer = Producer::from_hosts(vec![self.host_addr.to_string()])
                .create()
                .map_err(|err| NodeStreamProducerError::UnableToCreateProducer { err })?;
        }
        self.kafka_producer
            .send(&record)
            .map_err(|err| NodeStreamProducerError::MessageSendFailed { err })
    }

    pub fn clone(&self) -> Result<Self, NodeStreamProducerError> {
        Ok(Self {
            host_addr: self.host_addr,
            kafka_producer: Producer::from_hosts(vec![self.host_addr.to_string()])
                .create()
                .map_err(|err| NodeStreamProducerError::UnableToCreateProducer { err })?,
        })
    }
}
