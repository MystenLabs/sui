use std::{
    collections::{BTreeMap, HashMap},
    time::Duration,
};

use anyhow::Result;
use futures::StreamExt;
use libp2p::{
    gossipsub::{
        self, Behaviour as Gossipsub, Event as GossipsubEvent, IdentTopic,
        Message as GossipsubMessage, MessageAuthenticity, MessageId, ValidationMode,
    },
    identify,
    identity::Keypair,
    mdns,
    swarm::{NetworkBehaviour, SwarmEvent},
    Multiaddr, PeerId, SwarmBuilder,
};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use super::MedianPrice;

const PRICE_TOPIC: &str = "v1/oracle/price";

/// Runs the P2P node and returns the handle (used to broadcast price to the network
/// or stop the P2P node) along with the receiver of the consensus prices, mainly used
/// in the API so we can update the price of the asset.
pub async fn setup_p2p() -> Result<(P2PNodeHandle, mpsc::Receiver<MedianPrice>)> {
    let (consensus_tx, consensus_rx) = mpsc::channel(32);

    let (mut node, command_tx) = P2PNode::new(consensus_tx).await?;
    let handle = P2PNodeHandle { command_tx };

    tokio::spawn(async move {
        if let Err(e) = node.run().await {
            tracing::error!("P2P node error: {}", e);
        }
    });
    Ok((handle, consensus_rx))
}

#[derive(NetworkBehaviour)]
struct OracleBehaviour {
    gossipsub: Gossipsub,
    mdns: mdns::tokio::Behaviour,
    identify: identify::Behaviour,
}

pub struct P2PNodeHandle {
    command_tx: mpsc::UnboundedSender<NodeCommand>,
}

#[derive(Debug)]
pub enum NodeCommand {
    BroadcastPrice(MedianPrice, u64),
    Shutdown,
}

impl P2PNodeHandle {
    pub async fn broadcast_price(&self, price: MedianPrice, checkpoint: u64) -> Result<()> {
        self.command_tx
            .send(NodeCommand::BroadcastPrice(price, checkpoint))
            .map_err(|e| anyhow::anyhow!("Failed to send broadcast command: {}", e))?;
        Ok(())
    }

    pub async fn shutdown(&self) -> Result<()> {
        self.command_tx
            .send(NodeCommand::Shutdown)
            .map_err(|e| anyhow::anyhow!("Failed to send shutdown command: {}", e))?;
        Ok(())
    }
}

pub struct P2PNode {
    swarm: libp2p::Swarm<OracleBehaviour>,
    price_consensus: PriceConsensus,
    consensus_sender: mpsc::Sender<MedianPrice>,
    keypair: Keypair,
    command_rx: mpsc::UnboundedReceiver<NodeCommand>,
    price_topic: IdentTopic,
}

impl P2PNode {
    pub async fn new(
        consensus_sender: mpsc::Sender<MedianPrice>,
    ) -> Result<(Self, mpsc::UnboundedSender<NodeCommand>)> {
        let keypair = Keypair::generate_ed25519();
        let peer_id = PeerId::from(keypair.public());
        tracing::info!("🫣 Local peer id: {}", peer_id);

        let mut swarm = SwarmBuilder::with_existing_identity(keypair.clone())
            .with_tokio()
            .with_quic()
            .with_behaviour(|_| {
                // Set up Gossipsub with more aggressive parameters for better local networking
                let gossipsub_config = gossipsub::ConfigBuilder::default()
                    .validation_mode(ValidationMode::Strict)
                    .message_id_fn(|message: &GossipsubMessage| {
                        MessageId::from(message.data.clone())
                    })
                    .heartbeat_interval(Duration::from_secs(5))
                    .history_gossip(3)
                    .build()
                    .expect("Invalid GossipSub Config");

                let gossipsub = Gossipsub::new(
                    MessageAuthenticity::Signed(keypair.clone()),
                    gossipsub_config,
                )
                .unwrap();

                // Set up mDNS
                let mdns = mdns::tokio::Behaviour::new(mdns::Config::default(), peer_id)
                    .expect("Failed to create mDNS behaviour");

                let identify = identify::Behaviour::new(identify::Config::new(
                    "/oracle/1.0.0".into(),
                    keypair.public(),
                ));

                Ok(OracleBehaviour {
                    gossipsub,
                    mdns,
                    identify,
                })
            })?
            .build();

        let listen_addr: Multiaddr = "/ip4/0.0.0.0/udp/0/quic-v1".parse()?;
        swarm.listen_on(listen_addr)?;
        let local_addr: Multiaddr = "/ip4/127.0.0.1/udp/0/quic-v1".parse()?;
        swarm.listen_on(local_addr)?;

        let topic = gossipsub::IdentTopic::new(PRICE_TOPIC);
        swarm.behaviour_mut().gossipsub.subscribe(&topic)?;

        let (command_tx, command_rx) = mpsc::unbounded_channel();

        let node = Self {
            swarm,
            price_consensus: PriceConsensus::new(),
            consensus_sender,
            keypair,
            command_rx,
            price_topic: topic,
        };

        Ok((node, command_tx))
    }

    pub async fn run(&mut self) -> Result<()> {
        loop {
            tokio::select! {
                Some(cmd) = self.command_rx.recv() => {
                    match cmd {
                        NodeCommand::BroadcastPrice(price, checkpoint) => {
                            let _ = self.broadcast_price(price, checkpoint).await;
                        }
                        NodeCommand::Shutdown => break,
                    }
                }
                event = self.swarm.select_next_some() => {
                    if let Err(e) = self.handle_swarm_event(event).await {
                        tracing::error!(%e, "Failed to handle swarm event");
                    }
                }
            }
        }

        Ok(())
    }

    async fn handle_swarm_event(&mut self, event: SwarmEvent<OracleBehaviourEvent>) -> Result<()> {
        match event {
            SwarmEvent::Behaviour(behaviour) => self.handle_swarm_behaviour(behaviour).await?,
            SwarmEvent::NewListenAddr { address, .. } => {
                tracing::info!(address = %address, "New listen address");
            }
            SwarmEvent::ConnectionEstablished {
                peer_id, endpoint, ..
            } => {
                tracing::info!(%peer_id, ?endpoint, "Connected to peer");
            }
            SwarmEvent::ConnectionClosed { peer_id, .. } => {
                tracing::info!(%peer_id, "Connection closed with peer");
            }
            _ => {}
        }

        Ok(())
    }

    async fn handle_swarm_behaviour(&mut self, behaviour: OracleBehaviourEvent) -> Result<()> {
        match behaviour {
            OracleBehaviourEvent::Gossipsub(GossipsubEvent::Message { message, .. }) => {
                self.handle_gossip_message(message).await?;
            }
            OracleBehaviourEvent::Mdns(mdns::Event::Discovered(peers)) => {
                for (peer_id, addr) in peers {
                    tracing::info!("🔍 mDNS discovered peer: {} at {}", peer_id, addr);
                    self.swarm
                        .behaviour_mut()
                        .gossipsub
                        .add_explicit_peer(&peer_id);
                }
            }
            OracleBehaviourEvent::Mdns(mdns::Event::Expired(peers)) => {
                for (peer_id, addr) in peers {
                    tracing::info!("❌ mDNS peer expired: {} at {}", peer_id, addr);
                    self.swarm
                        .behaviour_mut()
                        .gossipsub
                        .remove_explicit_peer(&peer_id);
                }
            }
            OracleBehaviourEvent::Identify(identify::Event::Received { peer_id, info, .. }) => {
                tracing::info!(
                    "👋 Identified peer {} with {} protocols",
                    peer_id,
                    info.protocols.len()
                );
                if info
                    .protocols
                    .iter()
                    .any(|p| p.to_string().contains("gossipsub"))
                {
                    self.swarm
                        .behaviour_mut()
                        .gossipsub
                        .add_explicit_peer(&peer_id);
                }
            }
            _ => {}
        }
        Ok(())
    }

    async fn handle_gossip_message(&mut self, message: gossipsub::Message) -> Result<()> {
        let price: SignedPrice = serde_json::from_slice(&message.data)?;

        // Ignore our own messages
        if price.peer_id == self.swarm.local_peer_id().to_string() {
            return Ok(());
        }

        // Ignore old messages
        if price.checkpoint < self.price_consensus.earliest_checkpoint() {
            return Ok(());
        }

        // Process the price and check for consensus
        if let Some((checkpoint, consensus_price)) = self.price_consensus.add_price(price) {
            tracing::info!(checkpoint, "Reached consensus!");
            self.consensus_sender
                .send(consensus_price)
                .await
                .map_err(|_| anyhow::anyhow!("Failed to send consensus price"))?;
        }

        Ok(())
    }

    async fn broadcast_price(&mut self, price: MedianPrice, checkpoint: u64) -> Result<()> {
        let signature = self.keypair.sign(&bcs::to_bytes(&price)?)?;

        let signed_price = SignedPrice {
            price,
            signature,
            peer_id: self.swarm.local_peer_id().to_string(),
            checkpoint,
        };
        let message = serde_json::to_vec(&signed_price)?;

        self.swarm
            .behaviour_mut()
            .gossipsub
            .publish(self.price_topic.clone(), message)?;
        self.price_consensus.add_price(signed_price);

        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SignedPrice {
    pub price: super::MedianPrice,
    pub signature: Vec<u8>,
    pub peer_id: String,
    pub checkpoint: u64,
}

/// Data associated with each checkpoint
#[derive(Debug, Default)]
pub struct CheckpointData {
    prices: Vec<SignedPrice>,
    price_counts: HashMap<u128, usize>,
}

impl CheckpointData {
    /// Gets the latest timestamp from all prices
    fn get_latest_timestamp(&self) -> Option<u64> {
        self.prices
            .iter()
            .map(|p| p.price.timestamp)
            .max()
            .expect("Should not be None")
    }
}

#[derive(Debug)]
pub struct PriceConsensus(pub BTreeMap<u64, CheckpointData>);

impl PriceConsensus {
    /// Creates a new PriceConsensus instance
    pub fn new() -> Self {
        Self(BTreeMap::new())
    }

    /// Returns the lowest checkpoint in the map or 0 if the map is empty.
    pub fn earliest_checkpoint(&self) -> u64 {
        self.0.first_key_value().map(|(key, _)| *key).unwrap_or(0)
    }

    /// Adds a price into the gathered prices from the network.
    /// Returns Some((checkpoint, price)) if consensus was reached, None otherwise.
    pub fn add_price(&mut self, signed_price: SignedPrice) -> Option<(u64, MedianPrice)> {
        let checkpoint_data = self.0.entry(signed_price.checkpoint).or_default();

        if checkpoint_data
            .prices
            .iter()
            .any(|p| p.peer_id == signed_price.peer_id)
        {
            return None;
        }

        let count = checkpoint_data
            .price_counts
            .entry(signed_price.price.median_price.expect("Should not be None"))
            .or_default();
        *count += 1;

        checkpoint_data.prices.push(signed_price.clone());

        let consensus = if *count >= 2 {
            let consensus_price = MedianPrice {
                pair: "BTC/USD".to_string(),
                median_price: signed_price.price.median_price,
                timestamp: checkpoint_data.get_latest_timestamp(),
            };
            Some((signed_price.checkpoint, consensus_price))
        } else {
            None
        };

        if consensus.is_some() {
            self.cleanup_old_checkpoints(signed_price.checkpoint);
        }

        consensus
    }

    fn cleanup_old_checkpoints(&mut self, current_checkpoint: u64) {
        self.0
            .retain(|&checkpoint, _| checkpoint > current_checkpoint);
    }
}
