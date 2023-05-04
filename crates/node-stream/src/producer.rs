// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{sync::Arc, time::Duration};

use kafka::producer::{Producer, Record};

use crate::types::NodeStreamData;

pub struct NodeStreamProducer {
    host_addr: String,
    kafka_producer: Producer,
}
impl NodeStreamProducer {
    pub fn new(host_addr: String) -> Self {
        let kafka_producer = Producer::from_hosts(vec![host_addr.clone()])
            .create()
            .unwrap(); // TODO: handle error
        Self {
            host_addr,
            kafka_producer,
        }
    }

    pub fn send(&mut self, data: NodeStreamData) {
        let (topic, bytes) = data.decompose();
        let record = Record::from_value(topic.as_str(), bytes);

        let mut num_trials = 100;
        // TODO:
        // Handle errors
        // Check that topic exists before sending

        // Make sure up to date
        self.kafka_producer
            .client_mut()
            .load_metadata_all()
            .unwrap();

        // This will only happen first time this topis is hit
        while !self
            .kafka_producer
            .client()
            .topics()
            .contains(topic.as_str())
            && num_trials > 0
        {
            // Add the topic
            self.kafka_producer
                .client_mut()
                .load_metadata(&[topic.as_str()])
                .unwrap();
            num_trials -= 1;
            std::thread::sleep(Duration::from_millis(100));
            // try to initialize again
            self.kafka_producer = Producer::from_hosts(vec![self.host_addr.clone()])
                .create()
                .unwrap();
        }
        self.kafka_producer.send(&record).unwrap(); // TODO: handle error
    }

    pub fn duplicate(&self, id: String) -> Self {
        Self {
            host_addr: self.host_addr.clone(),
            kafka_producer: Producer::from_hosts(vec![self.host_addr.clone()])
                .create()
                .unwrap(), // TODO: handle error
        }
    }
}
