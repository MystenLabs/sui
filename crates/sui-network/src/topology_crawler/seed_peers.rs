use crate::topology_crawler::peer_source::PeerTarget;

#[derive(Clone, Copy, Debug)]
pub enum Network {
    Mainnet,
    Testnet,
}

pub fn seed_peers(_network: Network) -> Vec<PeerTarget> {
    todo!("hardcoded seed peers from sui-full-node.mdx")
}
