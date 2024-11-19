pub mod codec;

use codec::*;
use dashmap::DashMap;
use libp2p::request_response::ProtocolSupport;
use libp2p::swarm::dial_opts::{DialOpts, PeerCondition};

use std::collections::{BTreeMap, HashMap};

use anyhow::Result;
use futures::StreamExt;
use libp2p::{
    identify,
    identity::Keypair,
    mdns,
    request_response::{
        self, Behaviour as RequestResponse, Event as RequestResponseEvent,
        Message as RequestResponseMessage,
    },
    swarm::{NetworkBehaviour, SwarmEvent},
    Multiaddr, PeerId, SwarmBuilder,
};
use serde::{Deserialize, Serialize};

use tokio::sync::mpsc::{self, error::TrySendError};

use super::MedianPrice;

/// Runs the P2P node and returns the handle (used to broadcast price to the network
/// or stop the P2P node) along with the receiver of the consensus prices, mainly used
/// in the API so we can update the price of the asset.
pub async fn setup_p2p() -> Result<(P2PNodeHandle, mpsc::Receiver<MedianPrice>)> {
    let (consensus_tx, consensus_rx) = mpsc::channel(1024);

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
    request_response: RequestResponse<SignedPriceExchangeCodec>,
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
    peers: DashMap<PeerId, Multiaddr>,
}

impl P2PNode {
    pub async fn new(
        consensus_sender: mpsc::Sender<MedianPrice>,
    ) -> Result<(Self, mpsc::UnboundedSender<NodeCommand>)> {
        let keypair = Keypair::generate_ed25519();
        let peer_id = PeerId::from(keypair.public());

        let mut swarm = SwarmBuilder::with_existing_identity(keypair.clone())
            .with_tokio()
            .with_quic()
            .with_behaviour(|_| {
                let request_response = RequestResponse::new(
                    std::iter::once((SignedPriceExchangeProtocol(), ProtocolSupport::Full)),
                    request_response::Config::default(),
                );

                let mdns = mdns::tokio::Behaviour::new(mdns::Config::default(), peer_id)
                    .expect("Failed to create mDNS behaviour");

                let identify = identify::Behaviour::new(identify::Config::new(
                    "/pragma/oracle/1.0.0".into(),
                    keypair.public(),
                ));

                Ok(OracleBehaviour {
                    request_response,
                    mdns,
                    identify,
                })
            })?
            .build();

        let listen_addr: Multiaddr = "/ip4/0.0.0.0/udp/0/quic-v1".parse()?;
        swarm.listen_on(listen_addr)?;
        let local_addr: Multiaddr = "/ip4/127.0.0.1/udp/0/quic-v1".parse()?;
        swarm.listen_on(local_addr)?;

        let (command_tx, command_rx) = mpsc::unbounded_channel();

        let node = Self {
            swarm,
            price_consensus: PriceConsensus::new(),
            consensus_sender,
            keypair,
            command_rx,
            peers: DashMap::new(),
        };

        Ok((node, command_tx))
    }

    pub async fn run(&mut self) -> Result<()> {
        loop {
            tokio::select! {
                Some(cmd) = self.command_rx.recv() => {
                    match cmd {
                        NodeCommand::BroadcastPrice(price, checkpoint) => {
                            if let Err(e) = self.broadcast_price(price, checkpoint).await {
                                tracing::error!("Failed to broadcast price: {}", e);
                            }
                        }
                        NodeCommand::Shutdown => break,
                    }
                }
                event = self.swarm.select_next_some() => {
                    if let Err(e) = self.handle_swarm_event(event).await {
                        tracing::error!("Failed to handle swarm event: {}", e);
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
            _ => {}
        }

        Ok(())
    }

    async fn handle_swarm_behaviour(&mut self, behaviour: OracleBehaviourEvent) -> Result<()> {
        match behaviour {
            OracleBehaviourEvent::RequestResponse(RequestResponseEvent::Message {
                message,
                ..
            }) => {
                if let Err(e) = self.handle_p2p_message(message).await {
                    tracing::error!(%e, "Failed to handle gossip message");
                }
            }
            OracleBehaviourEvent::Mdns(mdns::Event::Discovered(peers)) => {
                for (peer_id, addr) in peers {
                    let dial_opts = DialOpts::peer_id(peer_id)
                        .condition(PeerCondition::DisconnectedAndNotDialing)
                        .addresses(vec![addr.clone()])
                        .build();
                    let _ = self.swarm.dial(dial_opts);
                    self.peers.insert(peer_id, addr);
                }
            }
            OracleBehaviourEvent::Mdns(mdns::Event::Expired(peers)) => {
                for peer in peers {
                    let _ = self.swarm.disconnect_peer_id(peer.0);
                    self.peers.remove(&peer.0);
                }
            }
            OracleBehaviourEvent::Identify(identify::Event::Received { peer_id, info, .. }) => {
                if &peer_id == self.swarm.local_peer_id() {
                    return Ok(());
                }
                // Ignore peers that are not using the Signed Price Protocol
                if &info.protocol_version != SignedPriceExchangeProtocol().as_ref() {
                    tracing::info!("DELETING PEER");
                    let _ = self.swarm.disconnect_peer_id(peer_id);
                    self.peers.remove(&peer_id);
                }
            }
            _ => {}
        }
        Ok(())
    }

    async fn handle_p2p_message(
        &mut self,
        message: RequestResponseMessage<SignedPrice, ()>,
    ) -> Result<()> {
        let price: SignedPrice = match message {
            RequestResponseMessage::Request { request, .. } => request,
            _ => return Ok(()),
        };

        // TODO: Verify signature
        // TODO: Ask other node that they really have the price
        if price.checkpoint < self.price_consensus.earliest_checkpoint() {
            return Ok(());
        }

        if let Some((checkpoint, consensus_price)) = self.price_consensus.add_price(price) {
            tracing::info!(checkpoint, "Reached consensus");

            match self.consensus_sender.try_send(consensus_price) {
                Ok(_) => (),
                Err(TrySendError::Full(_)) => {
                    tracing::warn!(checkpoint, "Consensus channel full, dropping price update");
                }
                Err(TrySendError::Closed(_)) => {
                    return Err(anyhow::anyhow!("Consensus channel closed"));
                }
            }
        }
        Ok(())
    }

    async fn broadcast_price(&mut self, price: MedianPrice, checkpoint: u64) -> Result<()> {
        if self.peers.is_empty() {
            return Ok(());
        }

        let signed_price = SignedPrice {
            signature: self.keypair.sign(&bcs::to_bytes(&price)?)?,
            price,
            peer_id: self.swarm.local_peer_id().to_string(),
            checkpoint,
        };

        for peer in self.peers.iter() {
            self.swarm
                .behaviour_mut()
                .request_response
                .send_request(&peer.key(), signed_price.clone());
        }
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
