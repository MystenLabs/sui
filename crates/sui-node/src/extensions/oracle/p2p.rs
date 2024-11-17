use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use anyhow::Result;
use futures::StreamExt;
use libp2p::{
    gossipsub::{
        self, Behaviour as Gossipsub, Event as GossipsubEvent, Message as GossipsubMessage,
        MessageAuthenticity, MessageId, ValidationMode,
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

const PRICE_TOPIC: &str = "oracle/price/v1";

pub async fn setup_p2p() -> Result<(P2PNodeHandle, mpsc::Receiver<MedianPrice>)> {
    let (consensus_tx, consensus_rx) = mpsc::channel(32);

    let (mut node, command_tx) = P2PNode::new(consensus_tx).await?;
    let handle = P2PNodeHandle { command_tx };

    // Start the node in a separate task
    tokio::spawn(async move {
        if let Err(e) = node.run().await {
            tracing::error!("P2P node error: {}", e);
        }
    });

    Ok((handle, consensus_rx))
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SignedPrice {
    pub price: super::MedianPrice,
    pub signature: Vec<u8>,
    pub peer_id: String,
    pub checkpoint: u64,
}

#[derive(Debug)]
struct CheckpointConsensus {
    prices: Vec<SignedPrice>,
    has_reached_consensus: bool,
}

impl CheckpointConsensus {
    fn new() -> Self {
        Self {
            prices: Vec::new(),
            has_reached_consensus: false,
        }
    }

    fn add_price(&mut self, price: SignedPrice) -> bool {
        if !self.prices.iter().any(|p| p.peer_id == price.peer_id) {
            self.prices.push(price);
        }
        self.has_quorum() && !self.has_reached_consensus
    }

    fn has_quorum(&self) -> bool {
        self.prices.len() >= 2
    }

    fn get_consensus_price(&mut self) -> Option<MedianPrice> {
        if !self.has_quorum() || self.has_reached_consensus {
            return None;
        }

        let mut price_counts = std::collections::HashMap::new();
        for price in &self.prices {
            *price_counts.entry(price.price.median_price).or_insert(0) += 1;
        }

        let consensus = price_counts
            .into_iter()
            .max_by_key(|&(_, count)| count)
            .map(|(price, _)| MedianPrice {
                pair: "BTC/USD".to_string(),
                median_price: price,
                timestamp: self.prices[0].price.timestamp,
            });

        if consensus.is_some() {
            self.has_reached_consensus = true;
        }

        consensus
    }
}

#[derive(Debug)]
pub struct PriceConsensus {
    checkpoints: std::collections::BTreeMap<u64, CheckpointConsensus>,
}

impl PriceConsensus {
    fn new() -> Self {
        Self {
            checkpoints: std::collections::BTreeMap::new(),
        }
    }

    fn add_price(&mut self, price: SignedPrice) -> bool {
        let checkpoint_consensus = self
            .checkpoints
            .entry(price.checkpoint)
            .or_insert_with(CheckpointConsensus::new);

        checkpoint_consensus.add_price(price)
    }

    // TODO: We should check that we always publish the most recent consensus.
    // If the published consensus is at N, never publish on the api any n-.
    fn get_consensus_price(&mut self) -> Option<(u64, MedianPrice)> {
        for (&checkpoint, consensus) in self.checkpoints.iter_mut() {
            if let Some(price) = consensus.get_consensus_price() {
                return Some((checkpoint, price));
            }
        }
        None
    }

    // Clean up old checkpoints periodically
    fn cleanup_old_checkpoints(&mut self, current_checkpoint: u64) {
        let old_threshold = current_checkpoint.saturating_sub(10); // Keep last 10 checkpoints
        self.checkpoints
            .retain(|&checkpoint, _| checkpoint >= old_threshold);
    }
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
    peer_count: Arc<AtomicUsize>,
    keypair: Keypair,
    command_rx: mpsc::UnboundedReceiver<NodeCommand>,
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
                    .heartbeat_interval(Duration::from_secs(1))
                    .history_length(10)
                    .history_gossip(3)
                    .mesh_n(6)
                    .mesh_n_low(4)
                    .mesh_n_high(12)
                    .flood_publish(true)
                    .build()
                    .expect("Valid config");

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

        // Listen on all interfaces with QUIC
        let listen_addr: Multiaddr = "/ip4/0.0.0.0/udp/0/quic-v1".parse()?;
        swarm.listen_on(listen_addr)?;

        // Also listen on localhost for better local discovery
        let local_addr: Multiaddr = "/ip4/127.0.0.1/udp/0/quic-v1".parse()?;
        swarm.listen_on(local_addr)?;

        let topic = gossipsub::IdentTopic::new(PRICE_TOPIC);
        swarm.behaviour_mut().gossipsub.subscribe(&topic)?;

        let peer_count = Arc::new(AtomicUsize::new(0));
        let (command_tx, command_rx) = mpsc::unbounded_channel();

        let node = Self {
            swarm,
            price_consensus: PriceConsensus::new(),
            consensus_sender,
            peer_count,
            keypair,
            command_rx,
        };

        Ok((node, command_tx))
    }

    pub async fn run(&mut self) -> Result<()> {
        let mut cleanup_interval = tokio::time::interval(Duration::from_secs(3));
        loop {
            tokio::select! {
                _ = cleanup_interval.tick() => {
                    if let Some((&latest_checkpoint, _)) = self.price_consensus.checkpoints.iter().next_back() {
                        self.price_consensus.cleanup_old_checkpoints(latest_checkpoint);
                    }
                }
                Some(cmd) = self.command_rx.recv() => {
                    match cmd {
                        NodeCommand::BroadcastPrice(price, checkpoint) => {
                            if let Err(e) = self.broadcast_price_internal(price, checkpoint).await {
                                tracing::error!("Failed to broadcast price: {}", e);
                            }
                        }
                        NodeCommand::Shutdown => break,
                    }
                }
                event = self.swarm.select_next_some() => match event {
                    SwarmEvent::Behaviour(behaviour) => match behaviour {
                        OracleBehaviourEvent::Gossipsub(GossipsubEvent::Message { message, .. }) => {
                            if let Ok(price) = serde_json::from_slice::<SignedPrice>(&message.data) {
                                if self.price_consensus.add_price(price.clone()) {
                                    if let Some((checkpoint, consensus_price)) = self.price_consensus.get_consensus_price() {
                                        tracing::info!(
                                            "🎯 Reached consensus for checkpoint {} with {} peers",
                                            checkpoint,
                                            self.price_consensus.checkpoints
                                                .get(&checkpoint)
                                                .map(|c| c.prices.len())
                                                .unwrap_or(0)
                                        );
                                        self.consensus_sender.send(consensus_price).await?;
                                    }
                                }
                            }
                        }
                        OracleBehaviourEvent::Mdns(mdns::Event::Discovered(peers)) => {
                            for (peer_id, addr) in peers {
                                tracing::info!("🔍 mDNS discovered peer: {} at {}", peer_id, addr);
                                self.swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
                                let count = self.peer_count.fetch_add(1, Ordering::SeqCst) + 1;
                                tracing::info!("Total peers: {}", count);
                            }
                        }
                        OracleBehaviourEvent::Mdns(mdns::Event::Expired(peers)) => {
                            for (peer_id, addr) in peers {
                                tracing::info!("❌ mDNS peer expired: {} at {}", peer_id, addr);
                                self.swarm.behaviour_mut().gossipsub.remove_explicit_peer(&peer_id);
                                let count = self.peer_count.fetch_sub(1, Ordering::SeqCst) - 1;
                                tracing::info!("Total peers: {}", count);
                            }
                        }
                        OracleBehaviourEvent::Identify(identify::Event::Received { peer_id, info, .. }) => {
                            tracing::info!("👋 Identified peer {} with {} protocols", peer_id, info.protocols.len());
                            if info.protocols.iter().any(|p| p.to_string().contains("gossipsub")) {
                                self.swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
                            }
                        }
                        _ => {}
                    },
                    SwarmEvent::NewListenAddr { address, .. } => {
                        tracing::info!("📡 Listening on {:?}", address);
                    }
                    SwarmEvent::ConnectionEstablished { peer_id, endpoint, .. } => {
                        tracing::info!("🤝 Connected to peer: {} via {:?}", peer_id, endpoint);
                    }
                    SwarmEvent::ConnectionClosed { peer_id, .. } => {
                        tracing::info!("👋 Connection closed with peer: {}", peer_id);
                    }
                    _ => {}
                }
            }
        }

        Ok(())
    }

    async fn broadcast_price_internal(
        &mut self,
        price: MedianPrice,
        checkpoint: u64,
    ) -> Result<()> {
        let signature = self.keypair.sign(&bcs::to_bytes(&price)?)?;
        tracing::info!("✍️  Signed price for checkpoint {}", checkpoint);

        let signed_price = SignedPrice {
            price,
            signature,
            peer_id: self.swarm.local_peer_id().to_string(),
            checkpoint,
        };

        let message = serde_json::to_vec(&signed_price)?;
        self.price_consensus.add_price(signed_price);
        let topic = gossipsub::IdentTopic::new(PRICE_TOPIC);

        self.swarm
            .behaviour_mut()
            .gossipsub
            .publish(topic, message)?;
        tracing::info!("📢 Broadcasted price to network");

        Ok(())
    }
}
