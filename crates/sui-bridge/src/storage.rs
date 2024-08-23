// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use sui_types::Identifier;

use sui_types::event::EventID;
use typed_store::rocks::{DBMap, MetricConf};
use typed_store::traits::TableSummary;
use typed_store::traits::TypedStoreDebug;
use typed_store::DBMapUtils;
use typed_store::Map;

use crate::error::{BridgeError, BridgeResult};
use crate::types::{BridgeAction, BridgeActionDigest};

#[derive(DBMapUtils)]
pub struct BridgeOrchestratorTables {
    /// pending BridgeActions that orchestrator received but not yet executed
    pub(crate) pending_actions: DBMap<BridgeActionDigest, BridgeAction>,
    /// module identifier to the last processed EventID
    pub(crate) sui_syncer_cursors: DBMap<Identifier, EventID>,
    /// contract address to the last processed block
    pub(crate) eth_syncer_cursors: DBMap<ethers::types::Address, u64>,
}

impl BridgeOrchestratorTables {
    pub fn new(path: &Path) -> Arc<Self> {
        Arc::new(Self::open_tables_read_write(
            path.to_path_buf(),
            MetricConf::new("bridge"),
            None,
            None,
        ))
    }

    pub(crate) fn insert_pending_actions(&self, actions: &[BridgeAction]) -> BridgeResult<()> {
        let mut batch = self.pending_actions.batch();
        batch
            .insert_batch(
                &self.pending_actions,
                actions.iter().map(|a| (a.digest(), a)),
            )
            .map_err(|e| {
                BridgeError::StorageError(format!("Couldn't insert into pending_actions: {:?}", e))
            })?;
        batch
            .write()
            .map_err(|e| BridgeError::StorageError(format!("Couldn't write batch: {:?}", e)))
    }

    pub(crate) fn remove_pending_actions(
        &self,
        actions: &[BridgeActionDigest],
    ) -> BridgeResult<()> {
        let mut batch = self.pending_actions.batch();
        batch
            .delete_batch(&self.pending_actions, actions)
            .map_err(|e| {
                BridgeError::StorageError(format!("Couldn't delete from pending_actions: {:?}", e))
            })?;
        batch
            .write()
            .map_err(|e| BridgeError::StorageError(format!("Couldn't write batch: {:?}", e)))
    }

    pub(crate) fn update_sui_event_cursor(
        &self,
        module: Identifier,
        cursor: EventID,
    ) -> BridgeResult<()> {
        let mut batch = self.sui_syncer_cursors.batch();

        batch
            .insert_batch(&self.sui_syncer_cursors, [(module, cursor)])
            .map_err(|e| {
                BridgeError::StorageError(format!(
                    "Coudln't insert into sui_syncer_cursors: {:?}",
                    e
                ))
            })?;
        batch
            .write()
            .map_err(|e| BridgeError::StorageError(format!("Couldn't write batch: {:?}", e)))
    }

    pub(crate) fn update_eth_event_cursor(
        &self,
        contract_address: ethers::types::Address,
        cursor: u64,
    ) -> BridgeResult<()> {
        let mut batch = self.eth_syncer_cursors.batch();

        batch
            .insert_batch(&self.eth_syncer_cursors, [(contract_address, cursor)])
            .map_err(|e| {
                BridgeError::StorageError(format!(
                    "Coudln't insert into eth_syncer_cursors: {:?}",
                    e
                ))
            })?;
        batch
            .write()
            .map_err(|e| BridgeError::StorageError(format!("Couldn't write batch: {:?}", e)))
    }

    pub fn get_all_pending_actions(&self) -> HashMap<BridgeActionDigest, BridgeAction> {
        self.pending_actions.unbounded_iter().collect()
    }

    pub fn get_sui_event_cursors(
        &self,
        identifiers: &[Identifier],
    ) -> BridgeResult<Vec<Option<EventID>>> {
        self.sui_syncer_cursors.multi_get(identifiers).map_err(|e| {
            BridgeError::StorageError(format!("Couldn't get sui_syncer_cursors: {:?}", e))
        })
    }

    pub fn get_eth_event_cursors(
        &self,
        contract_addresses: &[ethers::types::Address],
    ) -> BridgeResult<Vec<Option<u64>>> {
        self.eth_syncer_cursors
            .multi_get(contract_addresses)
            .map_err(|e| {
                BridgeError::StorageError(format!("Couldn't get sui_syncer_cursors: {:?}", e))
            })
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use sui_types::digests::TransactionDigest;

    use crate::test_utils::get_test_sui_to_eth_bridge_action;

    use super::*;

    // async: existing runtime is required with typed-store
    #[tokio::test]
    async fn test_bridge_storage_basic() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = BridgeOrchestratorTables::new(temp_dir.path());

        let action1 = get_test_sui_to_eth_bridge_action(
            None,
            Some(0),
            Some(99),
            Some(10000),
            None,
            None,
            None,
        );

        let action2 = get_test_sui_to_eth_bridge_action(
            None,
            Some(2),
            Some(100),
            Some(10000),
            None,
            None,
            None,
        );

        // in the beginning it's empty
        let actions = store.get_all_pending_actions();
        assert!(actions.is_empty());

        // remove non existing entry is ok
        store.remove_pending_actions(&[action1.digest()]).unwrap();

        store
            .insert_pending_actions(&vec![action1.clone(), action2.clone()])
            .unwrap();

        let actions = store.get_all_pending_actions();
        assert_eq!(
            actions,
            HashMap::from_iter(vec![
                (action1.digest(), action1.clone()),
                (action2.digest(), action2.clone())
            ])
        );

        // insert an existing action is ok
        store.insert_pending_actions(&[action1.clone()]).unwrap();
        let actions = store.get_all_pending_actions();
        assert_eq!(
            actions,
            HashMap::from_iter(vec![
                (action1.digest(), action1.clone()),
                (action2.digest(), action2.clone())
            ])
        );

        // remove action 2
        store.remove_pending_actions(&[action2.digest()]).unwrap();
        let actions = store.get_all_pending_actions();
        assert_eq!(
            actions,
            HashMap::from_iter(vec![(action1.digest(), action1.clone())])
        );

        // remove action 1
        store.remove_pending_actions(&[action1.digest()]).unwrap();
        let actions = store.get_all_pending_actions();
        assert!(actions.is_empty());

        // update eth event cursor
        let eth_contract_address = ethers::types::Address::random();
        let eth_block_num = 199999u64;
        assert!(store
            .get_eth_event_cursors(&[eth_contract_address])
            .unwrap()[0]
            .is_none());
        store
            .update_eth_event_cursor(eth_contract_address, eth_block_num)
            .unwrap();
        assert_eq!(
            store
                .get_eth_event_cursors(&[eth_contract_address])
                .unwrap()[0]
                .unwrap(),
            eth_block_num
        );

        // update sui event cursor
        let sui_module = Identifier::from_str("test").unwrap();
        let sui_cursor = EventID {
            tx_digest: TransactionDigest::random(),
            event_seq: 1,
        };
        assert!(store.get_sui_event_cursors(&[sui_module.clone()]).unwrap()[0].is_none());
        store
            .update_sui_event_cursor(sui_module.clone(), sui_cursor)
            .unwrap();
        assert_eq!(
            store.get_sui_event_cursors(&[sui_module.clone()]).unwrap()[0].unwrap(),
            sui_cursor
        );
    }
}
