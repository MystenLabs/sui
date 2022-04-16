// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use bytes::Bytes;
use futures::stream::futures_unordered::FuturesUnordered;
use futures::stream::StreamExt;
use futures::SinkExt;
use rocksdb::{ColumnFamilyDescriptor, DBCompressionType, DBWithThreadMode, MultiThreaded};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use sui_network::transport::{MessageHandler, RwChannel};
use sui_types::base_types::SequenceNumber;
use sui_types::error::{SuiError, SuiResult};
use sui_types::messages::ConsensusOutput;
use sui_types::serialize::{deserialize_message, serialize_consensus_output, SerializedMessage};
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::time::sleep;
use tracing::log;
use typed_store::rocks::{DBMap, TypedStoreError};
use typed_store::traits::Map;

/// A mock single-process sequencer. It is not crash-safe (it has no storage) and should
/// only be used for testing.
pub struct Sequencer {
    /// The network address where to receive input messages.
    pub input_address: SocketAddr,
    /// The network address where to receive subscriber requests.
    pub subscriber_address: SocketAddr,
    /// The network buffer size.
    pub buffer_size: usize,
    /// The delay to wait before sequencing a message. This parameter is only required to
    /// emulates the consensus' latency.
    pub consensus_delay: Duration,
}

impl Sequencer {
    /// Spawn a new sequencer. The sequencer is made of a number of component each running
    /// in their own tokio task.
    pub async fn spawn(sequencer: Self, store_path: &Path) -> SuiResult<()> {
        let (tx_input, rx_input) = channel(100);
        let (tx_subscriber, rx_subscriber) = channel(100);

        // Load the persistent storage.
        let store = Arc::new(SequencerStore::open(store_path, None));

        // Spawn the sequencer core.
        let mut sequencer_core = SequencerCore::new(rx_input, rx_subscriber, store.clone())?;
        tokio::spawn(async move {
            sequencer_core.run(sequencer.consensus_delay).await;
        });

        // Spawn the server receiving input messages to order.
        tokio::spawn(async move {
            let input_server = InputServer { tx_input };
            sui_network::transport::spawn_server(
                &sequencer.input_address.to_string(),
                Arc::new(input_server),
                sequencer.buffer_size,
            )
            .await
            .unwrap()
            .join()
            .await
            .unwrap();
        });

        // Spawn the server receiving subscribers to the output of the sequencer.
        tokio::spawn(async move {
            let subscriber_server = SubscriberServer::new(tx_subscriber, store);
            sui_network::transport::spawn_server(
                &sequencer.subscriber_address.to_string(),
                Arc::new(subscriber_server),
                sequencer.buffer_size,
            )
            .await
            .unwrap()
            .join()
            .await
            .unwrap();
        });

        Ok(())
    }
}

/// The core of the sequencer, totally ordering input bytes.
pub struct SequencerCore {
    /// Receive users' certificates to sequence
    rx_input: Receiver<Bytes>,
    /// Communicate with subscribers to update with the output of the sequence.
    rx_subscriber: Receiver<SubscriberMessage>,
    /// Persistent storage to hold all consensus outputs. This task is the only one
    /// that writes to the store.
    store: Arc<SequencerStore>,
    /// The global consensus index.
    consensus_index: SequenceNumber,
    /// The current number of subscribers.
    subscribers_count: usize,
}

impl SequencerCore {
    /// The maximum number of subscribers.
    pub const MAX_SUBSCRIBERS: usize = 1_000;

    /// Create a new sequencer core instance.
    pub fn new(
        rx_input: Receiver<Bytes>,
        rx_subscriber: Receiver<SubscriberMessage>,
        store: Arc<SequencerStore>,
    ) -> SuiResult<Self> {
        let consensus_index = store.get_last_consensus_index()?;
        Ok(Self {
            rx_input,
            rx_subscriber,
            store,
            consensus_index,
            subscribers_count: 0,
        })
    }

    /// Simply wait for a fixed delay and then returns the input.
    async fn waiter(deliver: Bytes, delay: Duration) -> Bytes {
        sleep(delay).await;
        deliver
    }

    /// Main loop ordering input bytes.
    pub async fn run(&mut self, consensus_delay: Duration) {
        let mut waiting = FuturesUnordered::new();
        let mut subscribers = HashMap::new();
        loop {
            tokio::select! {
                // Receive bytes to order.
                Some(message) = self.rx_input.recv() => {
                    waiting.push(Self::waiter(message, consensus_delay));
                },

                // Receive subscribers to update with the sequencer's output.
                Some(message) = self.rx_subscriber.recv() => {
                    if self.subscribers_count < Self::MAX_SUBSCRIBERS {
                        let SubscriberMessage(sender, id) = message;
                        subscribers.insert(id, sender);
                        self.subscribers_count +=1 ;
                    }
                },

                // Bytes are ready to be delivered, notify the subscribers.
                Some(message) = waiting.next() => {
                    let output = ConsensusOutput {
                        message: message.to_vec(),
                        sequence_number: self.consensus_index,
                    };

                    // Store the sequenced message. If this fails, we do not notify and subscribers
                    // and effectively throw away the message. Liveness may be lost.
                    if let Err(e) = self.store.store_output(&output) {
                        log::error!("Failed to store consensus output: {e}");
                        continue;
                    }

                    // Increment the consensus index.
                    self.consensus_index = self.consensus_index.increment();

                    // Notify the subscribers of the new output. If a subscriber's channel is full
                    // (the subscriber is slow), we simply skip this output. The subscriber will
                    // eventually sync to catch up.
                    let mut to_drop = Vec::new();
                    for (id, subscriber) in &subscribers {
                        if subscriber.is_closed() {
                            to_drop.push(*id);
                            continue;
                        }
                        if subscriber.capacity() > 0 && subscriber.send(output.clone()).await.is_err() {
                            to_drop.push(*id);
                        }
                    }

                    // Cleanup the list subscribers that dropped connection.
                    for id in to_drop {
                        subscribers.remove(&id);
                        self.subscribers_count -= 1;
                    }
                }
            }
        }
    }
}

/// Define how the network server should handle incoming clients' certificates. This
/// is not got to stream many input transactions (benchmarks) as the task handling the
/// TCP connection blocks until a reply is ready.
struct InputServer {
    /// Send user transactions to the sequencer.
    pub tx_input: Sender<Bytes>,
}

#[async_trait]
impl<'a, Stream> MessageHandler<Stream> for InputServer
where
    Stream: 'static + RwChannel<'a> + Unpin + Send,
{
    async fn handle_messages(&self, mut stream: Stream) {
        loop {
            // Read the user's certificate.
            let buffer = match stream.stream().next().await {
                Some(Ok(buffer)) => buffer,
                Some(Err(e)) => {
                    log::warn!("Error while reading TCP stream: {e}");
                    break;
                }
                None => {
                    log::debug!("Connection dropped by the client");
                    break;
                }
            };

            // Send the certificate to the sequencer.
            if self.tx_input.send(buffer.freeze()).await.is_err() {
                panic!("Failed to sequence input bytes");
            }

            // Send an acknowledgment to the client.
            if stream.sink().send(Bytes::from("Ok")).await.is_err() {
                log::debug!("Failed to send ack to client");
                break;
            }
        }
    }
}

/// Represents the subscriber's unique id.
pub type SubscriberId = usize;

/// The messages sent by the subscriber server to the sequencer core to notify
/// the core of a new subscriber.
#[derive(Debug)]
pub struct SubscriberMessage(Sender<ConsensusOutput>, SubscriberId);

/// Define how the network server should handle incoming authorities sync requests.
/// The authorities are basically light clients of the sequencer. A real consensus
/// implementation would make sure to receive an ack from the authorities and retry
/// sending until the message is delivered. This is safety-critical.
struct SubscriberServer {
    /// Notify the sequencer's core of a new subscriber.
    pub tx_subscriber: Sender<SubscriberMessage>,
    /// Count the number of subscribers.
    counter: AtomicUsize,
    /// The persistent storage. It is only used to help subscribers to sync, this
    /// task never writes to the store.
    store: Arc<SequencerStore>,
}

impl SubscriberServer {
    /// The number of pending updates that the subscriber can hold in memory.
    pub const CHANNEL_SIZE: usize = 1_000;

    /// Create a new subscriber server.
    pub fn new(tx_subscriber: Sender<SubscriberMessage>, store: Arc<SequencerStore>) -> Self {
        Self {
            tx_subscriber,
            counter: AtomicUsize::new(0),
            store,
        }
    }

    /// Helper function loading from store the outputs missed by the subscriber (in the right order).
    async fn synchronize<'a, Stream>(
        &self,
        sequence_number: SequenceNumber,
        stream: &mut Stream,
    ) -> SuiResult<()>
    where
        Stream: 'static + RwChannel<'a> + Unpin + Send,
    {
        // TODO: Loading the missed outputs one by one may not be the most efficient. But we can't
        // load them all in memory at once (there may be a lot of missed outputs). We could do
        // this in chunks.
        let consensus_index = self.store.get_last_consensus_index()?;
        if sequence_number < consensus_index {
            let start: u64 = sequence_number.into();
            let stop: u64 = consensus_index.into();
            for i in start..=stop {
                let index = SequenceNumber::from(i);
                let message = self.store.get_output(&index)?.unwrap();
                let serialized = serialize_consensus_output(&message);
                if stream.sink().send(Bytes::from(serialized)).await.is_err() {
                    log::debug!("Connection dropped by subscriber");
                    break;
                }
            }
        }
        Ok(())
    }
}

#[async_trait]
impl<'a, Stream> MessageHandler<Stream> for SubscriberServer
where
    Stream: 'static + RwChannel<'a> + Unpin + Send,
{
    async fn handle_messages(&self, mut stream: Stream) {
        let (tx_output, mut rx_output) = channel(Self::CHANNEL_SIZE);
        let subscriber_id = self.counter.fetch_add(1, Ordering::SeqCst);

        // Notify the core of a new subscriber.
        self.tx_subscriber
            .send(SubscriberMessage(tx_output, subscriber_id))
            .await
            .expect("Failed to send new subscriber to core");

        // Interact with the subscriber.
        loop {
            tokio::select! {
                // Update the subscriber every time a certificate is sequenced.
                Some(message) = rx_output.recv() => {
                    let serialized = serialize_consensus_output(&message);
                    if stream.sink().send(Bytes::from(serialized)).await.is_err() {
                        log::debug!("Connection dropped by subscriber");
                        break;
                    }
                },

                // Receive sync requests form the subscriber.
                Some(buffer) = stream.stream().next() => match buffer {
                    Ok(buffer) => match deserialize_message(&*buffer) {
                        Ok((_, SerializedMessage::ConsensusSync(sync))) => {
                            if let Err(e) = self.synchronize(sync.sequence_number, &mut stream).await {
                                log::error!("{e}");
                                break;
                            }
                        }
                        Ok((_, _)) => {
                            log::warn!("{}", SuiError::UnexpectedMessage);
                            break;
                        }

                        Err(e) => {
                            log::warn!("Failed to deserialize consensus sync request {e}");
                            break;
                        }
                    },
                    Err(e) => {
                        log::warn!("Error while reading TCP stream: {e}");
                        break;
                    }
                }
            }
        }
    }
}

/// The persistent storage of the sequencer.
pub struct SequencerStore {
    /// All sequenced messages indexed by sequence number.
    outputs: DBMap<SequenceNumber, ConsensusOutput>,
}

impl SequencerStore {
    /// Open the consensus store.
    pub fn open<P: AsRef<Path>>(path: P, db_options: Option<rocksdb::Options>) -> Self {
        let row_cache = rocksdb::Cache::new_lru_cache(1_000_000).expect("Cache is ok");
        let mut options = db_options.unwrap_or_default();
        options.set_row_cache(&row_cache);
        options.set_table_cache_num_shard_bits(10);
        options.set_compression_type(DBCompressionType::None);

        let db = Self::open_cf_opts(
            &path,
            Some(options.clone()),
            &[("last_consensus_index", &options), ("outputs", &options)],
        )
        .expect("Cannot open DB.");

        Self {
            outputs: DBMap::reopen(&db, Some("outputs")).expect("Cannot open CF."),
        }
    }

    /// Helper function to open the store.
    fn open_cf_opts<P: AsRef<Path>>(
        path: P,
        db_options: Option<rocksdb::Options>,
        opt_cfs: &[(&str, &rocksdb::Options)],
    ) -> Result<Arc<DBWithThreadMode<MultiThreaded>>, TypedStoreError> {
        let mut options = db_options.unwrap_or_default();
        options.create_if_missing(true);
        options.create_missing_column_families(true);

        let mut cfs = DBWithThreadMode::<MultiThreaded>::list_cf(&options, &path)
            .ok()
            .unwrap_or_default();
        for cf_key in opt_cfs.iter().map(|(name, _)| name) {
            let key = (*cf_key).to_owned();
            if !cfs.contains(&key) {
                cfs.push(key);
            }
        }

        Ok(Arc::new(
            DBWithThreadMode::<MultiThreaded>::open_cf_descriptors(
                &options,
                /* primary */ &path.as_ref(),
                opt_cfs
                    .iter()
                    .map(|(name, opts)| ColumnFamilyDescriptor::new(*name, (*opts).clone())),
            )?,
        ))
    }

    /// Read the last consensus index from the store.
    pub fn get_last_consensus_index(&self) -> SuiResult<SequenceNumber> {
        self.outputs
            .iter()
            .skip_prior_to(&SequenceNumber::MAX)?
            .next()
            .map_or_else(|| Ok(SequenceNumber::default()), |(s, _)| Ok(s.increment()))
    }

    /// Stores a new consensus output.
    pub fn store_output(&self, output: &ConsensusOutput) -> SuiResult<()> {
        let mut write_batch = self.outputs.batch();
        write_batch = write_batch.insert_batch(
            &self.outputs,
            std::iter::once((output.sequence_number, output)),
        )?;
        write_batch.write().map_err(SuiError::from)
    }

    /// Load a specific output from storage.
    pub fn get_output(&self, index: &SequenceNumber) -> SuiResult<Option<ConsensusOutput>> {
        self.outputs.get(index).map_err(SuiError::from)
    }
}
