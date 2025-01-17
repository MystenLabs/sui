use std::collections::{BTreeMap, HashMap, HashSet};

use anyhow::Result;
use futures::StreamExt;
use libp2p::{
    identify,
    identity::Keypair,
    mdns,
    swarm::{NetworkBehaviour, SwarmEvent},
    PeerId, SwarmBuilder,
};
use libp2p_gossipsub::{
    self, Behaviour as Gossipsub, Event as GossipsubEvent, IdentTopic, MessageAuthenticity,
};
use serde::{Deserialize, Serialize};
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use tokio::sync::mpsc;

use super::MedianPrice;

const PROTOCOL_VERSION: &str = "pragma/1.0.0";
const ORACLE_TOPIC: &str = "pragma/defi_protocol_name";

/// Runs the P2P node and returns the handle (used to broadcast price to the network
/// or stop the P2P node) along with the receiver of the consensus prices, mainly used
/// in the API so we can update the price of the asset.
pub async fn start_p2p() -> Result<(P2PBroadcaster, mpsc::Receiver<(MedianPrice, Vec<SignedData<MedianPrice>>)>)> {
    let (consensus_tx, consensus_rx) = mpsc::channel(1024);
    let (mut node, command_tx) = P2PNode::new(consensus_tx).await?;
    tracing::info!("[Oracle ExEx] 👤 Joined the Oracle P2P network");
    let handle = P2PBroadcaster(command_tx);
    tokio::spawn(async move { node.run().await });
    Ok((handle, consensus_rx))
}

pub type BroadcastedPrice = (MedianPrice, u64);

#[derive(Debug)]
pub struct P2PBroadcaster(mpsc::UnboundedSender<BroadcastedPrice>);

impl P2PBroadcaster {
    pub async fn broadcast(&self, price: MedianPrice, checkpoint: u64) -> Result<()> {
        self.0
            .send((price, checkpoint))
            .map_err(|e| anyhow::anyhow!("Failed to send broadcast to P2P: {}", e))?;
        Ok(())
    }
}

#[derive(NetworkBehaviour)]
struct OracleBehaviour {
    gossipsub: Gossipsub,
    mdns: mdns::tokio::Behaviour,
    identify: identify::Behaviour,
}

pub struct P2PNode {
    swarm: libp2p::Swarm<OracleBehaviour>,
    /// Topic where the P2P nodes will communicate their signed prices.
    oracle_topic: IdentTopic,
    /// Keypair of the current node. Used to sign prices.
    keypair: Keypair,
    /// History of the prices received in the network. Used to establish a quorum.
    network_prices: NetworkPricesPerCheckpoint,
    /// Allows to send prices that reached a quorum to the API.
    quorum_sender: mpsc::Sender<(MedianPrice, Vec<SignedData<MedianPrice>>)>,
    /// Channel used to retrieve aggregated median prices for a given checkpoint.
    command_rx: mpsc::UnboundedReceiver<BroadcastedPrice>,
    /// Peers currently connected.
    peers: HashSet<PeerId>,
}

impl P2PNode {
    pub async fn new(
        quorum_sender: mpsc::Sender<(MedianPrice, Vec<SignedData<MedianPrice>>)>,
    ) -> Result<(Self, mpsc::UnboundedSender<BroadcastedPrice>)> {
        let keypair = Keypair::generate_ed25519();
        let peer_id = PeerId::from(keypair.public());

        let mut swarm = SwarmBuilder::with_existing_identity(keypair.clone())
            .with_tokio()
            .with_quic()
            .with_behaviour(|_| {
                let gossipsub_config = libp2p_gossipsub::ConfigBuilder::default().build()?;
                let gossipsub = Gossipsub::new(
                    MessageAuthenticity::Signed(keypair.clone()),
                    gossipsub_config,
                )?;

                let mdns = mdns::tokio::Behaviour::new(mdns::Config::default(), peer_id)?;

                let identify = identify::Behaviour::new(identify::Config::new(
                    PROTOCOL_VERSION.to_string(),
                    keypair.public(),
                ));

                Ok(OracleBehaviour {
                    gossipsub,
                    mdns,
                    identify,
                })
            })?
            .build();
        swarm.listen_on("/ip4/0.0.0.0/udp/0/quic-v1".parse()?)?;
        swarm.listen_on("/ip4/127.0.0.1/udp/0/quic-v1".parse()?)?;

        let oracle_topic = libp2p_gossipsub::IdentTopic::new(ORACLE_TOPIC);
        swarm.behaviour_mut().gossipsub.subscribe(&oracle_topic)?;

        let (command_tx, command_rx) = mpsc::unbounded_channel();
        let node = Self {
            swarm,
            oracle_topic,
            keypair,
            network_prices: NetworkPricesPerCheckpoint::new(),
            quorum_sender,
            command_rx,
            peers: HashSet::default(),
        };

        Ok((node, command_tx))
    }

    pub async fn run(&mut self) {
        loop {
            tokio::select! {
                Some((median_price, checkpoint)) = self.command_rx.recv() => {
                    let _ = self.broadcast_price(median_price, checkpoint).await;
                }
                event = self.swarm.select_next_some() => {
                    self.handle_swarm_event(event).await;
                }
            }
        }
    }

    async fn handle_swarm_event(&mut self, event: SwarmEvent<OracleBehaviourEvent>) {
        match event {
            SwarmEvent::Behaviour(behaviour) => match behaviour {
                OracleBehaviourEvent::Gossipsub(GossipsubEvent::Message { message, .. }) => {
                    if let Err(e) = self.handle_p2p_message(message).await {
                        tracing::error!(%e, "[Oracle ExEx] Failed to handle gossip message");
                    }
                }
                OracleBehaviourEvent::Mdns(mdns::Event::Discovered(peers)) => {
                    for (peer_id, _) in peers {
                        self.swarm
                            .behaviour_mut()
                            .gossipsub
                            .add_explicit_peer(&peer_id);
                        self.peers.insert(peer_id);
                    }
                }
                OracleBehaviourEvent::Mdns(mdns::Event::Expired(peers)) => {
                    for (peer_id, _) in peers {
                        self.swarm
                            .behaviour_mut()
                            .gossipsub
                            .remove_explicit_peer(&peer_id);
                        self.peers.remove(&peer_id);
                    }
                }
                OracleBehaviourEvent::Identify(identify::Event::Received {
                    peer_id, info, ..
                }) => {
                    if info
                        .protocols
                        .iter()
                        .any(|p| p.to_string().contains("gossipsub"))
                    {
                        self.swarm
                            .behaviour_mut()
                            .gossipsub
                            .add_explicit_peer(&peer_id);
                        self.peers.insert(peer_id);
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }

    async fn handle_p2p_message(&mut self, message: libp2p_gossipsub::Message) -> Result<()> {
        let price: SignedData<MedianPrice> = bcs::from_bytes(&message.data)?;
        // TODO: Assert that the price was correctly signed by the peer
        self.add_price(price).await?;
        Ok(())
    }

    async fn broadcast_price(&mut self, price: MedianPrice, checkpoint: u64) -> Result<()> {
        let signed_price = SignedData::new(&self.keypair, &price, checkpoint)?;
        self.add_price(signed_price.clone()).await?;
        if self.peers.is_empty() {
            return Ok(());
        }
        self.swarm
            .behaviour_mut()
            .gossipsub
            .publish(self.oracle_topic.clone(), bcs::to_bytes(&signed_price)?)?;
        Ok(())
    }

    async fn add_price(&mut self, price: SignedData<MedianPrice>) -> anyhow::Result<()> {
        if let Some(quorum_reached_price) = self.network_prices.add_price(price, self.peers.len()) {
            // TODO: Add an attestation for the quorum so we can trace back who signed
            self.quorum_sender.send(quorum_reached_price).await?;
        }
        Ok(())
    }
}

/// Data associated with each checkpoint
#[derive(Debug, Default)]
pub struct CheckpointData {
    peer_prices: HashMap<String, SignedData<MedianPrice>>,
    price_counts: HashMap<u128, usize>,
    latest_timestamp: u64,
}

impl CheckpointData {
    fn new() -> Self {
        Self {
            peer_prices: HashMap::new(),
            price_counts: HashMap::new(),
            latest_timestamp: 0,
        }
    }

    fn add_peer_price(&mut self, signed_data: &SignedData<MedianPrice>) -> bool {
        if self.peer_prices.contains_key(&signed_data.peer_id) {
            return false;
        }

        if let Some(timestamp) = signed_data.price.timestamp {
            self.latest_timestamp = self.latest_timestamp.max(timestamp);
        }

        self.peer_prices
            .insert(signed_data.peer_id.clone(), signed_data.clone());
        true
    }

    fn get_latest_timestamp(&self) -> Option<u64> {
        if self.latest_timestamp > 0 {
            Some(self.latest_timestamp)
        } else {
            None
        }
    }
}

#[derive(Debug)]
pub struct NetworkPricesPerCheckpoint(pub BTreeMap<CheckpointSequenceNumber, CheckpointData>);

impl NetworkPricesPerCheckpoint {
    pub fn new() -> Self {
        Self(BTreeMap::new())
    }

    pub fn latest_checkpoint(&self) -> u64 {
        self.0.last_key_value().map(|(key, _)| *key).unwrap_or(0)
    }

    pub fn add_price(
        &mut self,
        signed_price: SignedData<MedianPrice>,
        current_number_of_peers: usize,
    ) -> Option<(MedianPrice, Vec<SignedData<MedianPrice>>)> {
        let Some(median_price) = signed_price.price.median_price else {
            return None;
        };

        let checkpoint_data = self
            .0
            .entry(signed_price.checkpoint)
            .or_insert_with(CheckpointData::new);

        if !checkpoint_data.add_peer_price(&signed_price) {
            return None;
        }

        let count = checkpoint_data
            .price_counts
            .entry(median_price)
            .and_modify(|c| *c += 1)
            .or_insert(1);

        let quorum_reached = *count >= Self::quorum(current_number_of_peers);
        if quorum_reached {
            let consensus_price = MedianPrice {
                pair: "BTC/USD".to_string(),
                median_price: Some(median_price),
                timestamp: checkpoint_data.get_latest_timestamp(),
                checkpoint: Some(signed_price.checkpoint),
            };
            let peers_data: Vec<SignedData<MedianPrice>> = self.0.get_mut(&signed_price.checkpoint).unwrap().peer_prices.iter().map(|(_, v)| v.clone()).collect();
            self.cleanup_old_checkpoints(signed_price.checkpoint);
            Some((consensus_price, peers_data))
        } else {
            None
        }
    }

    #[inline]
    fn quorum(current_number_of_peers: usize) -> usize {
        (current_number_of_peers + 1) / 2 + 1
    }

    fn cleanup_old_checkpoints(&mut self, current_checkpoint: u64) {
        self.0
            .retain(|&checkpoint, _| checkpoint > current_checkpoint);
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SignedData<T: Serialize> {
    pub peer_id: String,
    pub publickey: Vec<u8>,
    pub signature: Vec<u8>,
    pub checkpoint: u64,
    pub price: T,
}

impl<T: Serialize> SignedData<T> {
    pub fn new(signer: &Keypair, data: &T, checkpoint: u64) -> Result<SignedData<T>>
    where
        T: Serialize + Clone,
    {
        Ok(SignedData {
            peer_id: signer.public().to_peer_id().to_string(),
            publickey: signer.public().try_into_ed25519()?.to_bytes().to_vec(),
            signature: signer.sign(&bcs::to_bytes(data)?)?,
            checkpoint,
            price: data.clone(),
        })
    }
}
