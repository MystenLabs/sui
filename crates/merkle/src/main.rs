use std::mem::size_of;
use std::{path::Path, sync::Arc, time::Duration};

use anyhow::Result;

use merkle::tree::InMemoryStore;
use sui_storage::http_key_value_store::HttpKVStore;
use sui_storage::key_value_store_metrics::KeyValueStoreMetrics;
use sui_types::digests::TransactionDigest;
use sui_types::effects::TransactionEffects;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::messages_checkpoint::{
    CertifiedCheckpointSummary, CheckpointContents, CheckpointSequenceNumber,
};
use typed_store::traits::{Map, TableSummary, TypedStoreDebug};
use typed_store::{
    metrics::SamplingInterval,
    rocks::{
        default_db_options, read_size_from_env, DBBatch, DBMap, DBOptions, MetricConf,
        ReadWriteOptions,
    },
};
use typed_store_derive::DBMapUtils;

use itertools::Itertools;
use merkle::tree::{MerkleDigest, MerkleTree, MerkleTreeBuilder};
use merkle::tree::{Node, TreeStore};

#[tokio::main]
async fn main() -> Result<()> {
    println!("hello");

    let chain_data = Arc::new(ChainData::open("chaindata"));

    let http_store = HttpKVStore::new_kv(
        "https://transactions.sui.io/mainnet",
        KeyValueStoreMetrics::new_for_tests(),
    )?;

    let last_downloaded = chain_data
        .checkpoints
        .unbounded_iter()
        .skip_prior_to(&u64::MAX)?
        .next();

    let next = last_downloaded.map(|(i, _)| i + 1).unwrap_or(0);

    for chunk in &(next..60_000).chunks(100) {
        let n = chunk.collect_vec();
        let summaries = http_store.multi_get_checkpoints_summaries(&n).await?;

        let m = summaries
            .iter()
            .map(|c| c.as_ref().unwrap().content_digest)
            .collect_vec();

        let contents = http_store
            .multi_get_checkpoints_contents_by_digest(&m)
            .await?;

        let tx_digests = contents
            .iter()
            .flat_map(|c| c.as_ref().unwrap().iter())
            .map(|digests| digests.transaction)
            .collect_vec();

        let fxs = http_store.multi_get_fx_by_tx_digest(&tx_digests).await?;

        let mut batch = chain_data.checkpoints.batch();

        batch.insert_batch(
            &chain_data.checkpoints,
            n.clone()
                .into_iter()
                .zip(summaries.into_iter().map(Option::unwrap)),
        )?;

        batch.insert_batch(
            &chain_data.checkpoint_contents,
            n.clone()
                .into_iter()
                .zip(contents.into_iter().map(Option::unwrap)),
        )?;

        batch.insert_batch(
            &chain_data.fx,
            tx_digests
                .clone()
                .into_iter()
                .zip(fxs.into_iter().map(Option::unwrap)),
        )?;
        batch.write()?;

        println!("found and wrote: {:?}", n);
    }

    let merkledb = Arc::new(MerkleData::open("merkledb"));

    let last_computed = merkledb
        .state_root
        .unbounded_iter()
        .skip_prior_to(&u64::MAX)?
        .next();
    let next = last_computed.map(|(i, _)| i + 1).unwrap_or(0);

    for (checkpoint, contents) in chain_data
        .checkpoint_contents
        .unbounded_iter()
        .skip_prior_to(&next)?
    {
        let digests = contents
            .iter()
            .map(|digests| digests.transaction)
            .collect_vec();
        let fxs = chain_data
            .fx
            .multi_get(digests)?
            .into_iter()
            .map(|f| f.unwrap())
            .collect_vec();

        let mut builder = {
            if checkpoint == 0 {
                MerkleTree::new(ArcMerkleData(merkledb.clone())).into_builder()
            } else {
                let digest = merkledb.state_root.get(&(checkpoint - 1))?.unwrap();
                MerkleTree::with_root(ArcMerkleData(merkledb.clone()), digest)?.into_builder()
            }
        };

        for fx in fxs {
            // inserts
            for object_ref in fx.all_changed_objects() {
                builder.insert(object_ref.0)?;
            }
            // removals
            for id in fx.all_removed_objects() {
                builder.remove(id.0 .0)?;
            }
        }

        let tree = builder.build()?;
        merkledb
            .state_root
            .insert(&checkpoint, &tree.root().digest())?;
        println!(
            "done: {checkpoint}, object count: {}",
            tree.root().leaf_count()
        );
    }

    let mut db = InMemoryStore::new();
    for (digest, node) in merkledb.merkle_nodes.unbounded_iter() {
        db.write_node(node)?;
    }

    println!("db entries: {}", db.inner.len());
    println!("db size: {}", db.inner.len() * size_of::<Node>());
    let mut sizes = [0; 17];

    for node in db.inner.values() {
        sizes[node.child_count()] += 1;
    }

    for (i, count) in sizes.into_iter().enumerate() {
        println!("{i}: {count}");
    }

    Ok(())
}

#[derive(DBMapUtils)]
pub struct ChainData {
    pub(crate) checkpoints: DBMap<CheckpointSequenceNumber, CertifiedCheckpointSummary>,
    pub(crate) checkpoint_contents: DBMap<CheckpointSequenceNumber, CheckpointContents>,
    pub(crate) fx: DBMap<TransactionDigest, TransactionEffects>,
}

#[derive(DBMapUtils)]
pub struct MerkleData {
    pub(crate) state_root: DBMap<CheckpointSequenceNumber, MerkleDigest>,
    pub(crate) merkle_nodes: DBMap<MerkleDigest, Node>,
}

impl ChainData {
    pub fn open<T: AsRef<Path>>(path: T) -> Self {
        Self::open_tables_read_write(
            path.as_ref().into(),
            MetricConf::with_sampling(SamplingInterval::new(Duration::from_secs(60), 0)),
            None,
            None,
        )
    }
}

impl MerkleData {
    pub fn open<T: AsRef<Path>>(path: T) -> Self {
        Self::open_tables_read_write(
            path.as_ref().into(),
            MetricConf::with_sampling(SamplingInterval::new(Duration::from_secs(60), 0)),
            None,
            None,
        )
    }
}

struct ArcMerkleData(Arc<MerkleData>);

impl TreeStore for &MerkleData {
    fn get_node(&self, digest: MerkleDigest) -> anyhow::Result<Option<Node>> {
        self.merkle_nodes.get(&digest).map_err(Into::into)
    }

    fn write_node(&mut self, node: Node) -> anyhow::Result<()> {
        self.merkle_nodes
            .insert(&node.digest(), &node)
            .map_err(Into::into)
    }
}

impl TreeStore for ArcMerkleData {
    fn get_node(&self, digest: MerkleDigest) -> anyhow::Result<Option<Node>> {
        self.0.as_ref().get_node(digest)
    }

    fn write_node(&mut self, node: Node) -> anyhow::Result<()> {
        self.0.as_ref().write_node(node)
    }
}
