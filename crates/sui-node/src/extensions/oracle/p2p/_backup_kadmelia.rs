use std::{
    num::NonZero,
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
    kad::{
        store::MemoryStore, Behaviour as Kademlia, Config as KademliaConfig, Event as KademliaEvent,
    },
    swarm::{NetworkBehaviour, SwarmEvent},
    Multiaddr, PeerId, StreamProtocol, SwarmBuilder,
};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use super::MedianPrice;

const KADEMLIA_PROTOCOL: &str = "/subspace/kad/0.1.0";
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
pub struct PriceConsensus {
    prices: Vec<SignedPrice>,
    quorum_size: usize,
    peer_count: Arc<AtomicUsize>,
    current_checkpoint: Option<u64>,
}

impl PriceConsensus {
    fn new(peer_count: Arc<AtomicUsize>) -> Self {
        Self {
            prices: Vec::new(),
            quorum_size: 1,
            peer_count,
            current_checkpoint: None,
        }
    }

    fn add_price(&mut self, price: SignedPrice) -> bool {
        match self.current_checkpoint {
            Some(checkpoint) if checkpoint != price.checkpoint => {
                self.prices.clear();
                self.current_checkpoint = Some(price.checkpoint);
            }
            None => {
                self.current_checkpoint = Some(price.checkpoint);
            }
            _ => {}
        }

        self.quorum_size = std::cmp::max(
            1,
            (self.peer_count.load(Ordering::Relaxed) * 2 / 3) as usize,
        );

        if !self.prices.iter().any(|p| p.peer_id == price.peer_id) {
            self.prices.push(price);
        }

        self.has_quorum()
    }

    fn has_quorum(&self) -> bool {
        self.prices.len() >= self.quorum_size
    }

    fn get_consensus_price(&self) -> Option<super::MedianPrice> {
        if !self.has_quorum() {
            return None;
        }

        let mut price_counts = std::collections::HashMap::new();
        for price in &self.prices {
            *price_counts.entry(price.price.median_price).or_insert(0) += 1;
        }

        price_counts
            .into_iter()
            .max_by_key(|&(_, count)| count)
            .map(|(price, _)| super::MedianPrice {
                pair: "BTC/USD".to_string(),
                median_price: price,
                timestamp: self.prices[0].price.timestamp,
            })
    }
}

#[derive(NetworkBehaviour)]
struct OracleBehaviour {
    gossipsub: Gossipsub,
    kademlia: Kademlia<MemoryStore>,
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
    consensus_sender: mpsc::Sender<super::MedianPrice>,
    peer_count: Arc<AtomicUsize>,
    listen_addr: Option<Multiaddr>,
    keypair: Keypair,
    command_rx: mpsc::UnboundedReceiver<NodeCommand>,
}

impl P2PNode {
    pub async fn new(
        consensus_sender: mpsc::Sender<super::MedianPrice>,
    ) -> Result<(Self, mpsc::UnboundedSender<NodeCommand>)> {
        let keypair = Keypair::generate_ed25519();
        let peer_id = PeerId::from(keypair.public());
        tracing::info!("🫣 Local peer id: {}", peer_id);

        let mut swarm = SwarmBuilder::with_existing_identity(keypair.clone())
            .with_tokio()
            .with_quic()
            .with_behaviour(|_| {
                let store = MemoryStore::new(peer_id);
                let mut kademlia_config = KademliaConfig::new(
                    StreamProtocol::try_from_owned(KADEMLIA_PROTOCOL.to_owned())
                        .expect("Manual protocol name creation."),
                );
                kademlia_config
                    .set_query_timeout(Duration::from_secs(60))
                    .set_replication_factor(NonZero::new(3).unwrap())
                    .set_publication_interval(Some(Duration::from_secs(30)))
                    .set_record_ttl(Some(Duration::from_secs(120)))
                    .set_query_timeout(Duration::from_secs(15));

                let kademlia = Kademlia::with_config(peer_id, store, kademlia_config);

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

                let identify = identify::Behaviour::new(identify::Config::new(
                    "/oracle/1.0.0".into(),
                    keypair.public(),
                ));

                Ok(OracleBehaviour {
                    gossipsub,
                    kademlia,
                    identify,
                })
            })?
            .build();

        let listen_addrs: Vec<Multiaddr> = vec![
            "/ip4/0.0.0.0/udp/0/quic-v1".parse()?,
            "/ip6/::/udp/0/quic-v1".parse()?,
        ];
        for addr in &listen_addrs {
            if let Err(e) = swarm.listen_on(addr.clone()) {
                tracing::warn!("Failed to listen on {}: {}", addr, e);
            }
        }

        let topic = gossipsub::IdentTopic::new(PRICE_TOPIC);
        swarm.behaviour_mut().gossipsub.subscribe(&topic)?;

        let peer_count = Arc::new(AtomicUsize::new(0));
        let (command_tx, command_rx) = mpsc::unbounded_channel();

        let node = Self {
            swarm,
            price_consensus: PriceConsensus::new(peer_count.clone()),
            consensus_sender,
            peer_count,
            listen_addr: Some(listen_addrs[0].clone()), // Store the IPv4 address
            keypair,
            command_rx,
        };

        Ok((node, command_tx))
    }

    async fn broadcast_price_internal(
        &mut self,
        price: MedianPrice,
        checkpoint: u64,
    ) -> Result<()> {
        let signature = self.keypair.sign(&bcs::to_bytes(&price)?)?;

        let signed_price = SignedPrice {
            price,
            signature,
            peer_id: self.swarm.local_peer_id().to_string(),
            checkpoint,
        };

        let message = serde_json::to_vec(&signed_price)?;
        let topic = gossipsub::IdentTopic::new(PRICE_TOPIC);

        self.swarm
            .behaviour_mut()
            .gossipsub
            .publish(topic, message)?;

        Ok(())
    }

    async fn bootstrap_peers(&mut self) {
        if let Ok(addrs) = std::env::var("BOOTSTRAP_PEERS") {
            for addr in addrs.split(',') {
                if let Ok(addr) = addr.parse::<Multiaddr>() {
                    if let Some(peer_id) = get_peer_id_from_multiaddr(&addr) {
                        if addr.to_string().contains("quic") {
                            self.swarm
                                .behaviour_mut()
                                .kademlia
                                .add_address(&peer_id, addr.clone());
                            tracing::info!("Added QUIC bootstrap peer: {}", addr);
                        } else {
                            tracing::warn!("Skipping non-QUIC address: {}", addr);
                        }
                    }
                }
            }
        }

        match self.swarm.behaviour_mut().kademlia.bootstrap() {
            Ok(_) => tracing::info!("Started Kademlia bootstrap"),
            Err(e) => tracing::warn!("Failed to bootstrap Kademlia: {}", e),
        }

        let addresses: Vec<_> = self.swarm.listeners().cloned().collect();
        let local_peer_id = *self.swarm.local_peer_id();

        for addr in addresses {
            tracing::info!("🎧 Listening on {}", addr);
            self.swarm
                .behaviour_mut()
                .kademlia
                .add_address(&local_peer_id, addr);
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        self.bootstrap_peers().await;

        loop {
            tokio::select! {
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
                        OracleBehaviourEvent::Gossipsub(GossipsubEvent::Message {
                            message,
                            ..
                        }) => {
                            if let Ok(price) = serde_json::from_slice::<SignedPrice>(&message.data) {
                                if self.price_consensus.add_price(price.clone()) {
                                    tracing::info!(
                                        "Received price from peer {}: {:?}",
                                        price.peer_id,
                                        price.price
                                    );
                                    if let Some(consensus_price) = self.price_consensus.get_consensus_price() {
                                        self.consensus_sender.send(consensus_price).await?;
                                    }
                                }
                            }
                        }
                        OracleBehaviourEvent::Kademlia(KademliaEvent::RoutingUpdated { peer, .. }) => {
                            tracing::info!("🔄 Routing table updated for peer: {}", peer);
                            // Update peer count based on routing table
                            let new_count = self.swarm.behaviour_mut().kademlia.kbuckets().count();
                            self.peer_count.store(new_count, Ordering::SeqCst);
                            tracing::info!("📊 Current peer count: {}", new_count);
                        }
                        OracleBehaviourEvent::Identify(identify::Event::Received { peer_id, info, .. }) => {
                            tracing::info!("👋 Identified peer {} with protocols: {:?}", peer_id, info.protocols);
                            // Add all addresses to Kademlia
                            for addr in info.listen_addrs {
                                self.swarm.behaviour_mut().kademlia.add_address(&peer_id, addr);
                            }
                            if info.protocols.iter().any(|p| p.to_string().contains("gossipsub")) {
                                self.swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
                            }
                        }
                        _ => {}
                    },
                    SwarmEvent::NewListenAddr { address, .. } => {
                        tracing::info!("📡 Listening on {:?}", address);
                    }
                    SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                        tracing::info!("🤝 Connected to peer: {}", peer_id);
                    }
                    _ => {}
                }
            }
        }

        Ok(())
    }

    pub fn local_peer_id(&self) -> &PeerId {
        self.swarm.local_peer_id()
    }

    pub fn listen_addr(&self) -> Option<&Multiaddr> {
        self.listen_addr.as_ref()
    }
}

fn get_peer_id_from_multiaddr(addr: &Multiaddr) -> Option<PeerId> {
    use libp2p::multiaddr::Protocol;
    addr.iter().find_map(|p| match p {
        Protocol::P2p(peer_id) => Some(peer_id),
        _ => None,
    })
}
