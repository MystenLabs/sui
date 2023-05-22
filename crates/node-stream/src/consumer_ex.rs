// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::types_ex::{
    NodeStreamConsumerError, NodeStreamPayload, NodeStreamPerEpochTopic, NodeStreamSessionId,
};
use kafka::consumer::{Consumer, FetchOffset};
use std::marker::PhantomData;
use std::net::SocketAddr;
use tracing::info;
use tracing::{error, warn};

#[derive(Debug)]
pub struct NodeStreamConsumer<
    TopicType: NodeStreamPerEpochTopic<DataType, MetadataType>,
    DataType,
    MetadataType,
> where
    DataType: std::fmt::Debug,
    MetadataType: std::fmt::Debug,
{
    host_addr: SocketAddr,
    session: NodeStreamSessionId,
    kafka_consumer: Consumer,
    topic: TopicType,
    epoch: u64,
    phantom: PhantomData<DataType>,
    phantom2: PhantomData<MetadataType>,
}

impl<
        T: NodeStreamPerEpochTopic<D, M> + std::fmt::Debug,
        D: std::fmt::Debug,
        M: std::fmt::Debug,
    > NodeStreamConsumer<T, D, M>
{
    // Use sessions instead of IDs
    // Make it hard to clobber
    pub fn new(
        host_addr: SocketAddr,
        session_id: Option<NodeStreamSessionId>,
        epoch: u64,
        topic: T,
    ) -> Result<Self, NodeStreamConsumerError> {
        let topic_str = topic.topic_for_epoch(epoch).to_raw();
        let session = session_id.unwrap_or(NodeStreamSessionId::new());
        println!("Topic {}", topic_str);
        let kafka_consumer = Consumer::from_hosts(vec![host_addr.to_string()])
            .with_group(session.to_group_id())
            .with_topic(topic_str)
            .with_fallback_offset(FetchOffset::Earliest)
            .create()
            .map_err(|err| NodeStreamConsumerError::UnableToCreateConsumer { err })?;
        Ok(Self {
            host_addr,
            session,
            kafka_consumer,
            topic,
            epoch,
            phantom: PhantomData,
            phantom2: PhantomData,
        })
    }

    /// This will restart from the beginning of the stream.
    pub fn reset_with_session(
        self,
        session_id: NodeStreamSessionId,
    ) -> Result<Self, NodeStreamConsumerError> {
        if self.session == session_id {
            return Err(NodeStreamConsumerError::DuplicateSession {
                old: self.session,
                new: session_id,
            });
        }
        Self::new(self.host_addr, Some(self.session), self.epoch, self.topic)
    }

    pub fn poll(&mut self) -> Result<Vec<NodeStreamPayload<D, M>>, NodeStreamConsumerError> {
        let r = self
            .kafka_consumer
            .poll()
            .map_err(|err| NodeStreamConsumerError::UnableToPollMessage {
                topic: self.topic.topic_for_epoch(self.epoch),
                err,
            })?
            .iter()
            .map(|w| {
                let vals = w
                    .messages()
                    .iter()
                    .map(|m| {
                        self.topic.payload_from_bytes(m.value).map_err(|err| {
                            NodeStreamConsumerError::PayloadDeserializeError {
                                topic: self.topic.topic_for_epoch(self.epoch),
                                err: format!("{:?}", err),
                            }
                        })
                    })
                    .collect::<Result<Vec<_>, _>>();
                self.kafka_consumer.consume_messageset(w).map_err(|err| {
                    NodeStreamConsumerError::UnableToMarkMessageConsumed {
                        topic: self.topic.topic_for_epoch(self.epoch),
                        err,
                    }
                })?;
                vals
            })
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .collect();

        self.kafka_consumer.commit_consumed().map_err(|err| {
            NodeStreamConsumerError::UnableToCommitMessageConsumed {
                topic: self.topic.topic_for_epoch(self.epoch),
                err,
            }
        })?;
        Ok(r)
    }

    pub fn session_id(&self) -> NodeStreamSessionId {
        self.session
    }
}
