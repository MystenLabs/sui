// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::types::{NodeStreamData, NodeStreamTopic};
use kafka::consumer::{Consumer, FetchOffset};

#[derive(Debug)]
pub struct NodeStreamConsumer {
    host_addr: String,
    id: String,
    kafka_consumer: Consumer,
    topic: NodeStreamTopic,
    epoch: u64,
}

impl NodeStreamConsumer {
    pub fn new_with_id(host_addr: String, id: String, topic: NodeStreamTopic, epoch: u64) -> Self {
        let kafka_consumer = Consumer::from_hosts(vec![host_addr.clone()])
            .with_group(id.clone())
            .with_topic(topic.for_epoch(epoch))
            .with_fallback_offset(FetchOffset::Earliest)
            .create()
            .unwrap(); // TODO: handle error
        Self {
            host_addr,
            id,
            kafka_consumer,
            topic,
            epoch,
        }
    }

    /// This will restart from the beginning of the stream.
    pub fn reset_with_id(mut self, id: String) -> Self {
        assert!(self.id != id, "New ID must be different from old ID");
        self.id = id;
        let topic: String = self.topic.for_epoch(self.epoch);
        self.kafka_consumer = Consumer::from_hosts(vec![self.host_addr.clone()])
            .with_group(self.id.clone())
            .with_topic(topic)
            .with_fallback_offset(FetchOffset::Earliest)
            .create()
            .unwrap(); // TODO: handle error
        self
    }

    pub fn poll(&mut self) -> Vec<NodeStreamData> {
        // TODO: handle error
        let r = self
            .kafka_consumer
            .poll()
            .unwrap()
            .iter()
            .flat_map(|w| {
                let vals = w
                    .messages()
                    .iter()
                    .map(|m| bcs::from_bytes(m.value).unwrap());

                self.kafka_consumer.consume_messageset(w).unwrap();

                vals
            })
            .collect::<Vec<_>>();

        self.kafka_consumer.commit_consumed().unwrap();
        r
    }
}
