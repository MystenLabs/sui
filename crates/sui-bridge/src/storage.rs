// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::error::{BridgeError, BridgeResult};
use crate::types::{BridgeAction, BridgeActionDigest};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use sui_types::Identifier;
use sui_types::event::EventID;
use typed_store::DBMapUtils;
use typed_store::Map;
use typed_store::rocks::{DBMap, MetricConf};

#[derive(DBMapUtils)]
pub struct BridgeOrchestratorTables {
    /// pending BridgeActions that orchestrator received but not yet executed
    pub(crate) pending_actions: DBMap<BridgeActionDigest, BridgeAction>,
    /// module identifier to the last processed EventID
    pub(crate) sui_syncer_cursors: DBMap<Identifier, EventID>,
    /// contract address to the last processed block
    pub(crate) eth_syncer_cursors: DBMap<AlloyAddressSerializedAsEthers, u64>,
    /// sequence number for the next record to be processed from the bridge records table
    pub(crate) sui_syncer_sequence_number_cursor: DBMap<(), u64>,
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

    pub(crate) fn update_sui_sequence_number_cursor(&self, cursor: u64) -> BridgeResult<()> {
        let mut batch = self.sui_syncer_sequence_number_cursor.batch();

        batch
            .insert_batch(&self.sui_syncer_sequence_number_cursor, [((), cursor)])
            .map_err(|e| {
                BridgeError::StorageError(format!(
                    "Couldn't insert into sui_syncer_sequence_number_cursor: {:?}",
                    e
                ))
            })?;
        batch
            .write()
            .map_err(|e| BridgeError::StorageError(format!("Couldn't write batch: {:?}", e)))
    }

    pub(crate) fn update_eth_event_cursor(
        &self,
        contract_address: alloy::primitives::Address,
        cursor: u64,
    ) -> BridgeResult<()> {
        let mut batch = self.eth_syncer_cursors.batch();

        batch
            .insert_batch(
                &self.eth_syncer_cursors,
                [(AlloyAddressSerializedAsEthers(contract_address), cursor)],
            )
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
        self.pending_actions
            .safe_iter()
            .collect::<Result<HashMap<_, _>, _>>()
            .expect("failed to get all pending actions")
    }

    pub fn get_sui_event_cursors(
        &self,
        identifiers: &[Identifier],
    ) -> BridgeResult<Vec<Option<EventID>>> {
        self.sui_syncer_cursors.multi_get(identifiers).map_err(|e| {
            BridgeError::StorageError(format!("Couldn't get sui_syncer_cursors: {:?}", e))
        })
    }

    pub fn get_sui_sequence_number_cursor(&self) -> BridgeResult<Option<u64>> {
        self.sui_syncer_sequence_number_cursor
            .get(&())
            .map_err(|e| {
                BridgeError::StorageError(format!(
                    "Couldn't get sui_syncer_sequence_number_cursor: {:?}",
                    e
                ))
            })
    }

    pub fn get_eth_event_cursors(
        &self,
        contract_addresses: &[alloy::primitives::Address],
    ) -> BridgeResult<Vec<Option<u64>>> {
        let wrapped_addresses: Vec<AlloyAddressSerializedAsEthers> = contract_addresses
            .iter()
            .map(|addr| AlloyAddressSerializedAsEthers(*addr))
            .collect();
        self.eth_syncer_cursors
            .multi_get(&wrapped_addresses)
            .map_err(|e| {
                BridgeError::StorageError(format!("Couldn't get eth_syncer_cursors: {:?}", e))
            })
    }
}

/// Wrapper around alloy::primitives::Address that serializes in the same format
/// as ethers::types::Address (as a hex string) for backward compatibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AlloyAddressSerializedAsEthers(pub alloy::primitives::Address);

impl Serialize for AlloyAddressSerializedAsEthers {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let hex_string = format!("0x{:x}", self.0);
        hex_string.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for AlloyAddressSerializedAsEthers {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let address = s.parse().map_err(serde::de::Error::custom)?;
        Ok(AlloyAddressSerializedAsEthers(address))
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
        store
            .insert_pending_actions(std::slice::from_ref(&action1))
            .unwrap();
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
        let eth_contract_address = alloy::primitives::Address::random();
        let eth_block_num = 199999u64;
        assert!(
            store
                .get_eth_event_cursors(&[eth_contract_address])
                .unwrap()[0]
                .is_none()
        );
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
        assert!(
            store
                .get_sui_event_cursors(std::slice::from_ref(&sui_module))
                .unwrap()[0]
                .is_none()
        );
        store
            .update_sui_event_cursor(sui_module.clone(), sui_cursor)
            .unwrap();
        assert_eq!(
            store
                .get_sui_event_cursors(std::slice::from_ref(&sui_module))
                .unwrap()[0]
                .unwrap(),
            sui_cursor
        );

        // update sui seq cursor
        let sui_sequence_number_cursor = 100u64;
        assert!(store.get_sui_sequence_number_cursor().unwrap().is_none());
        store
            .update_sui_sequence_number_cursor(sui_sequence_number_cursor)
            .unwrap();
        assert_eq!(
            store.get_sui_sequence_number_cursor().unwrap().unwrap(),
            sui_sequence_number_cursor
        );
    }

    #[tokio::test]
    async fn test_address_serialization() {
        let alloy_address =
            alloy::primitives::Address::from_str("0x90f8bf6a479f320ead074411a4b0e7944ea8c9c1")
                .unwrap();
        let expected_ethers_serialized = vec![
            42, 0, 0, 0, 0, 0, 0, 0, 48, 120, 57, 48, 102, 56, 98, 102, 54, 97, 52, 55, 57, 102,
            51, 50, 48, 101, 97, 100, 48, 55, 52, 52, 49, 49, 97, 52, 98, 48, 101, 55, 57, 52, 52,
            101, 97, 56, 99, 57, 99, 49,
        ];
        let wrapped_address = AlloyAddressSerializedAsEthers(alloy_address);
        let alloy_serialized = bincode::serialize(&wrapped_address).unwrap();
        assert_eq!(alloy_serialized, expected_ethers_serialized);
    }

    #[tokio::test]
    async fn test_address_deserialization() {
        let ethers_serialized = vec![
            42, 0, 0, 0, 0, 0, 0, 0, 48, 120, 57, 48, 102, 56, 98, 102, 54, 97, 52, 55, 57, 102,
            51, 50, 48, 101, 97, 100, 48, 55, 52, 52, 49, 49, 97, 52, 98, 48, 101, 55, 57, 52, 52,
            101, 97, 56, 99, 57, 99, 49,
        ];
        let wrapped_address: AlloyAddressSerializedAsEthers =
            bincode::deserialize(&ethers_serialized).unwrap();
        let expected_address =
            alloy::primitives::Address::from_str("0x90f8bf6a479f320ead074411a4b0e7944ea8c9c1")
                .unwrap();
        assert_eq!(wrapped_address.0, expected_address);
    }
}
