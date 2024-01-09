use std::collections::{HashMap, HashSet};
use std::num::NonZeroUsize;
use std::sync::Arc;
use prometheus::Registry;
use sui_archival::reader::{ArchiveReader, ArchiveReaderMetrics};
use sui_config::node::ArchiveReaderConfig;
use sui_indexer::framework::{Handler, IndexerBuilder};
use sui_storage::object_store::ObjectStoreConfig;
use sui_types::error::{SuiError, SuiResult, UserInputError};
use sui_types::full_checkpoint_content::{CheckpointData, CheckpointTransaction};
use sui_types::messages_checkpoint::{CheckpointSequenceNumber, VerifiedCheckpoint, VerifiedCheckpointContents};
use sui_types::storage::{CheckpointHandler, ObjectKey};

pub struct ArchiveTailer {
    archive_config: Option<ObjectStoreConfig>,
    handlers: Vec<Box<dyn Handler>>,
    last_downloaded_checkpoint: Option<CheckpointSequenceNumber>,
    checkpoint_buffer_size: usize,
}

pub struct ArchiveCheckpointHandler {
    inner: Arc<dyn Handler>,
}

impl CheckpointHandler for ArchiveCheckpointHandler {
    fn handle_checkpoint(&mut self, verified_checkpoint: VerifiedCheckpoint, verified_checkpoint_contents: VerifiedCheckpointContents) -> Result<(), SuiError> {
        let checkpoint_data = CheckpointData {
            checkpoint_summary: verified_checkpoint.into(),
            checkpoint_contents: verified_checkpoint_contents.into_checkpoint_contents(),
            transactions: verified_checkpoint_contents.into_checkpoint_transactions(),
        };
    }
    fn make_checkpoint_data(verified_checkpoint: VerifiedCheckpoint, verified_checkpoint_contents: VerifiedCheckpointContents) -> anyhow::Result<CheckpointData> {
        let transaction_digests = verified_checkpoint_contents
            .into_checkpoint_contents()
            .iter()
            .map(|execution_digests| execution_digests.transaction)
            .collect::<Vec<_>>();

        let transactions: Vec<_> = verified_checkpoint_contents.iter().map(|x| x.transaction.clone()).collect();
        let effects: Vec<_> = verified_checkpoint_contents.iter().map(|x| x.effects.clone()).collect();
        let event_digests = effects
            .iter()
            .flat_map(|fx| fx.events_digest().copied())
            .collect::<Vec<_>>();

        let mut full_transactions = Vec::with_capacity(transactions.len());
        for (tx, fx) in transactions.into_iter().zip(effects) {
            // Note unwrapped_then_deleted contains **updated** versions.
            let unwrapped_then_deleted_obj_ids = fx
                .unwrapped_then_deleted()
                .into_iter()
                .map(|k| k.0)
                .collect::<HashSet<_>>();

            let input_object_keys = fx
                .input_shared_objects()
                .into_iter()
                .map(|kind| {
                    let (id, version) = kind.id_and_version();
                    ObjectKey(id, version)
                })
                .chain(
                    fx.modified_at_versions()
                        .into_iter()
                        .map(|(object_id, version)| ObjectKey(object_id, version)),
                )
                .collect::<HashSet<_>>()
                .into_iter()
                // Unwrapped-then-deleted objects are not stored in state before the tx, so we have nothing to fetch.
                .filter(|key| !unwrapped_then_deleted_obj_ids.contains(&key.0))
                .collect::<Vec<_>>();


            let output_object_keys = fx
                .all_changed_objects()
                .into_iter()
                .map(|(object_ref, _owner, _kind)| ObjectKey::from(object_ref))
                .collect::<Vec<_>>();

            let output_objects = state
                .multi_get_object_by_key(&output_object_keys)?
                .into_iter()
                .enumerate()
                .map(|(idx, maybe_object)| {
                    maybe_object.ok_or_else(|| {
                        anyhow::anyhow!(
                        "missing output object key {:?} from tx {}",
                        output_object_keys[idx],
                        tx.digest()
                    )
                    })
                })
                .collect::<anyhow::Result<Vec<_>>>()?;
        }








        for execution_data in verified_checkpoint_contents.iter() {
            let transaction = execution_data.transaction.clone();
            let effect = execution_data.effects.clone();

        }

    }
}
impl ArchiveTailer {
    const DEFAULT_CHECKPOINT_BUFFER_SIZE: usize = 1000;

    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            archive_config: None,
            handlers: Vec::new(),
            last_downloaded_checkpoint: None,
            checkpoint_buffer_size: Self::DEFAULT_CHECKPOINT_BUFFER_SIZE,
        }
    }

    pub fn archive_config(mut self, object_store_config: ObjectStoreConfig) -> Self {
        self.archive_config = Some(object_store_config);
        self
    }

    pub fn handler<T: Handler + 'static>(mut self, handler: T) -> Self {
        self.handlers.push(Box::new(handler));
        self
    }

    pub fn last_downloaded_checkpoint(
        mut self,
        last_downloaded_checkpoint: Option<CheckpointSequenceNumber>,
    ) -> Self {
        self.last_downloaded_checkpoint = last_downloaded_checkpoint;
        self
    }

    pub fn checkpoint_buffer_size(mut self, checkpoint_buffer_size: usize) -> Self {
        self.checkpoint_buffer_size = checkpoint_buffer_size;
        self
    }

    pub async fn run(self, registry: &Registry) {
        let archive_reader_meteric = Arc::new(ArchiveReaderMetrics::new(registry));
        let archive_reader_config = ArchiveReaderConfig {
            remote_store_config: self.archive_config.unwrap(),
            download_concurrency: NonZeroUsize::new(20).unwrap(),
            use_for_pruning_watermark: false
        };
        let archive_reader = ArchiveReader::new(archive_reader_config, &archive_reader_meteric)?;
        archive_reader.read(self.last_downloaded_checkpoint + 1, )