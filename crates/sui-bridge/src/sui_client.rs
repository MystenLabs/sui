// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// TODO remove when integrated
#![allow(unused)]

use std::str::from_utf8;
use std::time::Duration;

use anyhow::anyhow;
use async_trait::async_trait;
use axum::response::sse::Event;
use ethers::types::{Address, U256};
use fastcrypto::traits::KeyPair;
use fastcrypto::traits::ToFromBytes;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use sui_json_rpc_types::{EventFilter, Page, SuiData, SuiEvent};
use sui_json_rpc_types::{
    EventPage, SuiObjectDataOptions, SuiTransactionBlockResponse,
    SuiTransactionBlockResponseOptions,
};
use sui_sdk::{SuiClient as SuiSdkClient, SuiClientBuilder};
use sui_types::base_types::ObjectRef;
use sui_types::crypto::get_key_pair;
use sui_types::dynamic_field::Field;
use sui_types::error::UserInputError;
use sui_types::event;
use sui_types::gas_coin::GasCoin;
use sui_types::object::{Object, Owner};
use sui_types::transaction::Transaction;
use sui_types::{
    base_types::{ObjectID, SuiAddress},
    digests::TransactionDigest,
    event::EventID,
    Identifier,
};
use tap::TapFallible;
use tracing::warn;

use crate::crypto::BridgeAuthorityPublicKey;
use crate::error::{BridgeError, BridgeResult};
use crate::events::SuiBridgeEvent;
use crate::types::{
    BridgeAction, BridgeAuthority, BridgeCommittee, MoveTypeBridgeCommittee, MoveTypeBridgeInner,
    MoveTypeCommitteeMember,
};

// TODO: once we have bridge package on sui framework, we can hardcode the actual
// bridge dynamic field object id (not 0x9 or dynamic field wrapper) and update
// along with software upgrades.
// Or do we always retrieve from 0x9? We can figure this out before the first uggrade.
fn get_bridge_object_id() -> &'static ObjectID {
    static BRIDGE_OBJ_ID: OnceCell<ObjectID> = OnceCell::new();
    BRIDGE_OBJ_ID.get_or_init(|| {
        let bridge_object_id =
            std::env::var("BRIDGE_OBJECT_ID").expect("Expect BRIDGE_OBJECT_ID env var set");
        ObjectID::from_hex_literal(&bridge_object_id)
            .expect("BRIDGE_OBJECT_ID must be a valid hex string")
    })
}

pub struct SuiClient<P> {
    inner: P,
}

impl SuiClient<SuiSdkClient> {
    pub async fn new(rpc_url: &str) -> anyhow::Result<Self> {
        let inner = SuiClientBuilder::default().build(rpc_url).await?;
        let self_ = Self { inner };
        self_.describe().await?;
        Ok(self_)
    }
}

impl<P> SuiClient<P>
where
    P: SuiClientInner,
{
    pub fn new_for_testing(inner: P) -> Self {
        Self { inner }
    }

    // TODO assert chain identifier
    async fn describe(&self) -> anyhow::Result<()> {
        let chain_id = self.inner.get_chain_identifier().await?;
        let block_number = self.inner.get_latest_checkpoint_sequence_number().await?;
        tracing::info!(
            "SuiClient is connected to chain {chain_id}, current block number: {block_number}"
        );
        Ok(())
    }

    /// Query emitted Events that are defined in the given Move Module.
    /// It paginates results in **transaction granularity** for easier
    /// downstream processing. Unlike the native event query
    /// that uses `EventID` as cursor, here the cursor is `TransactionDigest`
    /// such that we can be sure the last event in the page must be the
    /// last Bridge event in that transaction.
    /// Consideration: a Sui transaction block should not contain too many
    /// Bridge events.
    pub async fn query_events_by_module(
        &self,
        package: ObjectID,
        module: Identifier,
        // Before we support query by checkpoint, we use `tx_digest`` as cursor.
        // Note the cursor is exclusive, so a callsite passing tx A as cursor
        // meaning it is intersted in events in transactions after A, globally ordering wise.
        cursor: TransactionDigest,
    ) -> BridgeResult<Page<SuiEvent, TransactionDigest>> {
        let filter = EventFilter::MoveEventModule { package, module };
        let initial_cursor = EventID {
            tx_digest: cursor,
            // Cursor is exclusive, so we use a reasonably large number
            // (when the code is written the max event num in a tx is 1024)
            // to skip the cursor tx entirely.
            event_seq: u16::MAX as u64,
        };
        let mut cursor = initial_cursor;
        let mut is_first_page = true;
        let mut all_events: Vec<sui_json_rpc_types::SuiEvent> = vec![];
        loop {
            let events = self.inner.query_events(filter.clone(), cursor).await?;
            if events.data.is_empty() {
                return Ok(Page {
                    data: all_events,
                    next_cursor: Some(cursor.tx_digest),
                    has_next_page: false,
                });
            }

            // unwrap safe: we just checked data is not empty
            let new_cursor = events.data.last().unwrap().id;

            // Now check if we need to query more events for the sake of
            // paginating in transaction granularity

            if !events.has_next_page {
                // A transaction's events shall be available all at once
                all_events.extend(events.data);
                return Ok(Page {
                    data: all_events,
                    next_cursor: Some(new_cursor.tx_digest),
                    has_next_page: false,
                });
            }

            if is_first_page {
                // the first page, take all returned events, go to next loop
                all_events.extend(events.data);
                cursor = new_cursor;
                is_first_page = false;
                continue;
            }

            // Not the first page, check if we collected all events in the tx
            let last_event_digest = events.data.last().map(|e| e.id.tx_digest);

            // We are done
            if last_event_digest != Some(cursor.tx_digest) {
                all_events.extend(
                    events
                        .data
                        .into_iter()
                        .take_while(|event| event.id.tx_digest == cursor.tx_digest),
                );
                return Ok(Page {
                    data: all_events,
                    next_cursor: Some(cursor.tx_digest),
                    has_next_page: true,
                });
            }

            // Returned events are all for the cursor tx and there are
            // potentially more, go to next loop.
            all_events.extend(events.data);
            cursor = new_cursor;
        }
    }

    pub async fn get_bridge_action_by_tx_digest_and_event_idx(
        &self,
        tx_digest: &TransactionDigest,
        event_idx: u16,
    ) -> BridgeResult<BridgeAction> {
        let events = self.inner.get_events_by_tx_digest(*tx_digest).await?;
        let event = events
            .get(event_idx as usize)
            .ok_or(BridgeError::NoBridgeEventsInTxPosition)?;
        let bridge_event = SuiBridgeEvent::try_from_sui_event(event)?
            .ok_or(BridgeError::NoBridgeEventsInTxPosition)?;

        bridge_event
            .try_into_bridge_action(*tx_digest, event_idx)
            .ok_or(BridgeError::BridgeEventNotActionable)
    }

    // TODO: expose this API to jsonrpc like system state query
    pub async fn get_bridge_committee(&self) -> BridgeResult<BridgeCommittee> {
        let move_type_bridge_committee =
            self.inner.get_bridge_committee().await.map_err(|e| {
                BridgeError::InternalError(format!("Can't get bridge committee: {e}"))
            })?;
        let mut authorities = vec![];
        // TODO: move this to MoveTypeBridgeCommittee
        for member in move_type_bridge_committee.members.contents {
            let MoveTypeCommitteeMember {
                sui_address,
                bridge_pubkey_bytes,
                voting_power,
                http_rest_url,
                blocklisted,
            } = member.value;
            let pubkey = BridgeAuthorityPublicKey::from_bytes(&bridge_pubkey_bytes)?;
            let base_url = from_utf8(&http_rest_url).unwrap_or_else(|e| {
                warn!(
                    "Bridge authority address: {}, pubkey: {:?} has invalid http url: {:?}",
                    sui_address, bridge_pubkey_bytes, http_rest_url
                );
                ""
            });
            authorities.push(BridgeAuthority {
                pubkey,
                voting_power,
                base_url: base_url.into(),
                is_blocklisted: blocklisted,
            });
        }
        BridgeCommittee::new(authorities)
    }

    pub async fn execute_transaction_block_with_effects(
        &self,
        tx: sui_types::transaction::Transaction,
    ) -> BridgeResult<SuiTransactionBlockResponse> {
        self.inner.execute_transaction_block_with_effects(tx).await
    }

    pub async fn get_gas_data_panic_if_not_gas(
        &self,
        gas_object_id: ObjectID,
    ) -> (GasCoin, ObjectRef, Owner) {
        self.inner
            .get_gas_data_panic_if_not_gas(gas_object_id)
            .await
    }

    pub async fn get_committee(&self) -> BridgeResult<BridgeCommittee> {
        self.get_bridge_committee().await
    }
}

/// Use a trait to abstract over the SuiSDKClient and SuiMockClient for testing.
#[async_trait]
pub trait SuiClientInner: Send + Sync {
    type Error: Into<anyhow::Error> + Send + Sync + std::error::Error + 'static;
    async fn query_events(
        &self,
        query: EventFilter,
        cursor: EventID,
    ) -> Result<EventPage, Self::Error>;

    async fn get_events_by_tx_digest(
        &self,
        tx_digest: TransactionDigest,
    ) -> Result<Vec<SuiEvent>, Self::Error>;

    async fn get_chain_identifier(&self) -> Result<String, Self::Error>;

    async fn get_latest_checkpoint_sequence_number(&self) -> Result<u64, Self::Error>;

    async fn get_bridge_committee(&self) -> Result<MoveTypeBridgeCommittee, Self::Error>;

    async fn execute_transaction_block_with_effects(
        &self,
        tx: Transaction,
    ) -> Result<SuiTransactionBlockResponse, BridgeError>;

    async fn get_gas_data_panic_if_not_gas(
        &self,
        gas_object_id: ObjectID,
    ) -> (GasCoin, ObjectRef, Owner);
}

#[async_trait]
impl SuiClientInner for SuiSdkClient {
    type Error = sui_sdk::error::Error;

    async fn query_events(
        &self,
        query: EventFilter,
        cursor: EventID,
    ) -> Result<EventPage, Self::Error> {
        self.event_api()
            .query_events(query, Some(cursor), None, false)
            .await
    }

    async fn get_events_by_tx_digest(
        &self,
        tx_digest: TransactionDigest,
    ) -> Result<Vec<SuiEvent>, Self::Error> {
        self.event_api().get_events(tx_digest).await
    }

    async fn get_chain_identifier(&self) -> Result<String, Self::Error> {
        self.read_api().get_chain_identifier().await
    }

    async fn get_latest_checkpoint_sequence_number(&self) -> Result<u64, Self::Error> {
        self.read_api()
            .get_latest_checkpoint_sequence_number()
            .await
    }

    async fn get_bridge_committee(&self) -> Result<MoveTypeBridgeCommittee, Self::Error> {
        let object_id = *get_bridge_object_id();
        let resp = self
            .read_api()
            .get_object_with_options(object_id, SuiObjectDataOptions::default().with_bcs())
            .await?;
        if resp.error.is_some() {
            return Err(Self::Error::DataError(format!(
                "Can't get bridge object {:?}: {:?}",
                object_id, resp.error
            )));
        }
        let move_object = resp
            .data
            .unwrap() // unwrap: Bridge object must exist
            .bcs
            .unwrap(); // unwrap requested bcs data
                       // unwrap: Bridge object must be a Move object
        let bcs = move_object.try_as_move().unwrap();
        let bridge_dynamic_field: Field<u64, MoveTypeBridgeInner> =
            bcs::from_bytes(&bcs.bcs_bytes)?;
        Ok(bridge_dynamic_field.value.committee)
    }

    async fn execute_transaction_block_with_effects(
        &self,
        tx: Transaction,
    ) -> Result<SuiTransactionBlockResponse, BridgeError> {
        match self.quorum_driver_api().execute_transaction_block(
            tx,
            SuiTransactionBlockResponseOptions::new().with_effects(),
            Some(sui_types::quorum_driver_types::ExecuteTransactionRequestType::WaitForEffectsCert),
        ).await {
            Ok(response) => Ok(response),
            Err(e) => return Err(BridgeError::SuiTxFailureGeneric(e.to_string())),
        }
    }

    async fn get_gas_data_panic_if_not_gas(
        &self,
        gas_object_id: ObjectID,
    ) -> (GasCoin, ObjectRef, Owner) {
        loop {
            match self
                .read_api()
                .get_object_with_options(
                    gas_object_id,
                    SuiObjectDataOptions::default().with_owner().with_content(),
                )
                .await
                .map(|resp| resp.data)
            {
                Ok(Some(gas_obj)) => {
                    let owner = gas_obj.owner.expect("Owner is requested");
                    let gas_coin = GasCoin::try_from(&gas_obj)
                        .unwrap_or_else(|err| panic!("{} is not a gas coin: {err}", gas_object_id));
                    return (gas_coin, gas_obj.object_ref(), owner);
                }
                other => {
                    warn!("Can't get gas object: {:?}: {:?}", gas_object_id, other);
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        events::EmittedSuiToEthTokenBridgeV1,
        sui_mock_client::SuiMockClient,
        types::{BridgeChainId, SuiToEthBridgeAction, TokenId},
    };
    use ethers::types::{
        Address, Block, BlockNumber, Filter, FilterBlockOption, Log, ValueOrArray, U64,
    };
    use prometheus::Registry;
    use std::{collections::HashSet, str::FromStr};

    use super::*;
    use crate::events::{init_all_struct_tags, SuiToEthTokenBridgeV1};

    #[tokio::test]
    async fn test_query_events_by_module() {
        // Note: for random events generated in this test, we only care about
        // tx_digest and event_seq, so it's ok that package and module does
        // not match the query parameters.
        telemetry_subscribers::init_for_testing();
        let mock_client = SuiMockClient::default();
        let sui_client = SuiClient::new_for_testing(mock_client.clone());
        let package = ObjectID::from_str("0xb71a9e").unwrap();
        let module = Identifier::from_str("BridgeTestModule").unwrap();

        // Case 1, empty response
        let mut cursor = TransactionDigest::random();
        let events = EventPage {
            data: vec![],
            next_cursor: None,
            has_next_page: false,
        };

        mock_client.add_event_response(
            package,
            module.clone(),
            EventID {
                tx_digest: cursor,
                event_seq: u16::MAX as u64,
            },
            events,
        );
        let page = sui_client
            .query_events_by_module(package, module.clone(), cursor)
            .await
            .unwrap();
        assert_eq!(
            page,
            Page {
                data: vec![],
                next_cursor: Some(cursor),
                has_next_page: false,
            }
        );
        // only one query
        assert_eq!(
            mock_client.pop_front_past_event_query_params().unwrap(),
            (
                package,
                module.clone(),
                EventID {
                    tx_digest: cursor,
                    event_seq: u16::MAX as u64
                }
            )
        );
        assert_eq!(mock_client.pop_front_past_event_query_params(), None);

        // Case 2, only one page (has_next_page = false)
        let event = SuiEvent::random_for_testing();
        let events = EventPage {
            data: vec![event.clone()],
            next_cursor: None,
            has_next_page: false,
        };
        mock_client.add_event_response(
            package,
            module.clone(),
            EventID {
                tx_digest: cursor,
                event_seq: u16::MAX as u64,
            },
            events,
        );
        let page = sui_client
            .query_events_by_module(package, module.clone(), cursor)
            .await
            .unwrap();
        assert_eq!(
            page,
            Page {
                data: vec![event.clone()],
                next_cursor: Some(event.id.tx_digest),
                has_next_page: false,
            }
        );
        // only one query
        assert_eq!(
            mock_client.pop_front_past_event_query_params().unwrap(),
            (
                package,
                module.clone(),
                EventID {
                    tx_digest: cursor,
                    event_seq: u16::MAX as u64
                }
            )
        );
        assert_eq!(mock_client.pop_front_past_event_query_params(), None);

        // Case 3, more than one pages, one tx has several events across pages
        // page 1 (event 1)
        let event_1 = SuiEvent::random_for_testing();
        let events_page_1 = EventPage {
            data: vec![event_1.clone()],
            next_cursor: Some(event_1.id),
            has_next_page: true,
        };
        mock_client.add_event_response(
            package,
            module.clone(),
            EventID {
                tx_digest: cursor,
                event_seq: u16::MAX as u64,
            },
            events_page_1,
        );
        // page 2 (event 1, event 2, same tx_digest)
        let mut event_2 = SuiEvent::random_for_testing();
        event_2.id.tx_digest = event_1.id.tx_digest;
        event_2.id.event_seq = event_1.id.event_seq + 1;
        let events_page_2 = EventPage {
            data: vec![event_2.clone()],
            next_cursor: Some(event_2.id),
            has_next_page: true,
        };
        mock_client.add_event_response(package, module.clone(), event_1.id, events_page_2);
        // page 3 (event 3, event 4, different tx_digest)
        let mut event_3 = SuiEvent::random_for_testing();
        event_3.id.tx_digest = event_2.id.tx_digest;
        event_3.id.event_seq = event_2.id.event_seq + 1;
        let event_4 = SuiEvent::random_for_testing();
        assert_ne!(event_3.id.tx_digest, event_4.id.tx_digest);
        let events_page_3 = EventPage {
            data: vec![event_3.clone(), event_4.clone()],
            next_cursor: Some(event_4.id),
            has_next_page: true,
        };
        mock_client.add_event_response(package, module.clone(), event_2.id, events_page_3);
        let page: Page<SuiEvent, TransactionDigest> = sui_client
            .query_events_by_module(package, module.clone(), cursor)
            .await
            .unwrap();
        // Get back event_1, event_2 and event_2 because of transaction level granularity
        assert_eq!(
            page,
            Page {
                data: vec![event_1.clone(), event_2.clone(), event_3.clone()],
                next_cursor: Some(event_2.id.tx_digest),
                has_next_page: true,
            }
        );
        // first page
        assert_eq!(
            mock_client.pop_front_past_event_query_params().unwrap(),
            (
                package,
                module.clone(),
                EventID {
                    tx_digest: cursor,
                    event_seq: u16::MAX as u64
                }
            )
        );
        // second page
        assert_eq!(
            mock_client.pop_front_past_event_query_params().unwrap(),
            (package, module.clone(), event_1.id)
        );
        // third page
        assert_eq!(
            mock_client.pop_front_past_event_query_params().unwrap(),
            (package, module.clone(), event_2.id)
        );
        // no more
        assert_eq!(mock_client.pop_front_past_event_query_params(), None);

        // Case 4, modify page 3 in case 3 to return event_4 only
        let events_page_3 = EventPage {
            data: vec![event_4.clone()],
            next_cursor: Some(event_4.id),
            has_next_page: true,
        };
        mock_client.add_event_response(package, module.clone(), event_2.id, events_page_3);
        let page: Page<SuiEvent, TransactionDigest> = sui_client
            .query_events_by_module(package, module.clone(), cursor)
            .await
            .unwrap();
        assert_eq!(
            page,
            Page {
                data: vec![event_1.clone(), event_2.clone()],
                next_cursor: Some(event_2.id.tx_digest),
                has_next_page: true,
            }
        );
        // first page
        assert_eq!(
            mock_client.pop_front_past_event_query_params().unwrap(),
            (
                package,
                module.clone(),
                EventID {
                    tx_digest: cursor,
                    event_seq: u16::MAX as u64
                }
            )
        );
        // second page
        assert_eq!(
            mock_client.pop_front_past_event_query_params().unwrap(),
            (package, module.clone(), event_1.id)
        );
        // third page
        assert_eq!(
            mock_client.pop_front_past_event_query_params().unwrap(),
            (package, module.clone(), event_2.id)
        );
        // no more
        assert_eq!(mock_client.pop_front_past_event_query_params(), None);

        // Case 5, modify page 2 in case 3 to mark has_next_page as false
        let events_page_2 = EventPage {
            data: vec![event_2.clone()],
            next_cursor: Some(event_2.id),
            has_next_page: false,
        };
        mock_client.add_event_response(package, module.clone(), event_1.id, events_page_2);
        let page: Page<SuiEvent, TransactionDigest> = sui_client
            .query_events_by_module(package, module.clone(), cursor)
            .await
            .unwrap();
        assert_eq!(
            page,
            Page {
                data: vec![event_1.clone(), event_2.clone()],
                next_cursor: Some(event_2.id.tx_digest),
                has_next_page: false,
            }
        );
        // first page
        assert_eq!(
            mock_client.pop_front_past_event_query_params().unwrap(),
            (
                package,
                module.clone(),
                EventID {
                    tx_digest: cursor,
                    event_seq: u16::MAX as u64
                }
            )
        );
        // second page
        assert_eq!(
            mock_client.pop_front_past_event_query_params().unwrap(),
            (package, module.clone(), event_1.id)
        );
        // no more
        assert_eq!(mock_client.pop_front_past_event_query_params(), None);
    }

    #[tokio::test]
    async fn get_bridge_action_by_tx_digest_and_event_idx() {
        // Note: for random events generated in this test, we only care about
        // tx_digest and event_seq, so it's ok that package and module does
        // not match the query parameters.
        telemetry_subscribers::init_for_testing();
        let mock_client = SuiMockClient::default();
        let sui_client = SuiClient::new_for_testing(mock_client.clone());
        let tx_digest = TransactionDigest::random();

        // Ensure all struct tags are inited
        init_all_struct_tags();
        let event_1 = EmittedSuiToEthTokenBridgeV1 {
            nonce: 1,
            sui_chain_id: BridgeChainId::SuiTestnet,
            sui_address: SuiAddress::random_for_testing_only(),
            eth_chain_id: BridgeChainId::EthSepolia,
            eth_address: Address::random(),
            token_id: TokenId::Sui,
            amount: 100,
        };
        let mut sui_event_1 = SuiEvent::random_for_testing();
        sui_event_1.type_ = SuiToEthTokenBridgeV1.get().unwrap().clone();
        sui_event_1.bcs = bcs::to_bytes(&event_1).unwrap();

        #[derive(Serialize, Deserialize)]
        struct RandomStruct {};

        let event_2: RandomStruct = RandomStruct {};
        // undeclared struct tag
        let mut sui_event_2 = SuiEvent::random_for_testing();
        sui_event_2.bcs = bcs::to_bytes(&event_2).unwrap();

        mock_client.add_events_by_tx_digest(
            tx_digest,
            vec![
                sui_event_1.clone(),
                sui_event_2.clone(),
                sui_event_1.clone(),
            ],
        );
        let mut expected_action_1 = BridgeAction::SuiToEthBridgeAction(SuiToEthBridgeAction {
            sui_tx_digest: tx_digest,
            sui_tx_event_index: 0,
            sui_bridge_event: event_1.clone(),
        });
        assert_eq!(
            sui_client
                .get_bridge_action_by_tx_digest_and_event_idx(&tx_digest, 0)
                .await
                .unwrap(),
            expected_action_1,
        );
        let mut expected_action_2 = BridgeAction::SuiToEthBridgeAction(SuiToEthBridgeAction {
            sui_tx_digest: tx_digest,
            sui_tx_event_index: 2,
            sui_bridge_event: event_1.clone(),
        });
        assert_eq!(
            sui_client
                .get_bridge_action_by_tx_digest_and_event_idx(&tx_digest, 2)
                .await
                .unwrap(),
            expected_action_2,
        );
        assert!(matches!(
            sui_client
                .get_bridge_action_by_tx_digest_and_event_idx(&tx_digest, 1)
                .await
                .unwrap_err(),
            BridgeError::NoBridgeEventsInTxPosition
        ),);
        assert!(matches!(
            sui_client
                .get_bridge_action_by_tx_digest_and_event_idx(&tx_digest, 3)
                .await
                .unwrap_err(),
            BridgeError::NoBridgeEventsInTxPosition
        ),);

        // if the StructTag matches with unparsable bcs, it returns an error
        sui_event_2.type_ = SuiToEthTokenBridgeV1.get().unwrap().clone();
        mock_client.add_events_by_tx_digest(tx_digest, vec![sui_event_2]);
        sui_client
            .get_bridge_action_by_tx_digest_and_event_idx(&tx_digest, 2)
            .await
            .unwrap_err();
    }
}
