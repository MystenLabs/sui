// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// TODO remove when integrated
#![allow(unused)]

use std::str::from_utf8;
use std::str::FromStr;
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
use sui_types::collection_types::LinkedTableNode;
use sui_types::crypto::get_key_pair;
use sui_types::dynamic_field::DynamicFieldName;
use sui_types::dynamic_field::Field;
use sui_types::error::SuiObjectResponseError;
use sui_types::error::UserInputError;
use sui_types::event;
use sui_types::gas_coin::GasCoin;
use sui_types::object::{Object, Owner};
use sui_types::transaction::Transaction;
use sui_types::TypeTag;
use sui_types::{
    base_types::{ObjectID, SuiAddress},
    digests::TransactionDigest,
    event::EventID,
    Identifier,
};
use tap::TapFallible;
use tracing::{error, warn};

use crate::crypto::BridgeAuthorityPublicKey;
use crate::error::{BridgeError, BridgeResult};
use crate::events::SuiBridgeEvent;
use crate::retry_with_max_elapsed_time;
use crate::sui_transaction_builder::get_bridge_package_id;
use crate::types::BridgeActionStatus;
use crate::types::BridgeInnerDynamicField;
use crate::types::BridgeRecordDyanmicField;
use crate::types::MoveTypeBridgeMessageKey;
use crate::types::MoveTypeBridgeRecord;
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

// object id of BridgeRecord, this is wrapped in the bridge inner object.
// TODO: once we have bridge package on sui framework, we can hardcode the actual id.
fn get_bridge_record_id() -> &'static ObjectID {
    static BRIDGE_RECORD_ID: OnceCell<ObjectID> = OnceCell::new();
    BRIDGE_RECORD_ID.get_or_init(|| {
        let bridge_record_id =
            std::env::var("BRIDGE_RECORD_ID").expect("Expect BRIDGE_RECORD_ID env var set");
        ObjectID::from_hex_literal(&bridge_record_id)
            .expect("BRIDGE_RECORD_ID must be a valid hex string")
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
    pub async fn query_events_by_module(
        &self,
        package: ObjectID,
        module: Identifier,
        // cursor is exclusive
        cursor: EventID,
    ) -> BridgeResult<Page<SuiEvent, EventID>> {
        let filter = EventFilter::MoveEventModule {
            package,
            module: module.clone(),
        };
        let events = self.inner.query_events(filter.clone(), cursor).await?;

        // Safeguard check that all events are emitted from requested package and module
        assert!(events
            .data
            .iter()
            .all(|event| event.type_.address.as_ref() == package.as_ref()
                && event.type_.module == module));
        Ok(events)
    }

    /// Returns BridgeAction from a Sui Transaction with transaction hash
    /// and the event index. If event is declared in an unrecognized
    /// package, return error.
    pub async fn get_bridge_action_by_tx_digest_and_event_idx_maybe(
        &self,
        tx_digest: &TransactionDigest,
        event_idx: u16,
    ) -> BridgeResult<BridgeAction> {
        let events = self.inner.get_events_by_tx_digest(*tx_digest).await?;
        let event = events
            .get(event_idx as usize)
            .ok_or(BridgeError::NoBridgeEventsInTxPosition)?;
        if event.type_.address.as_ref() != get_bridge_package_id().as_ref() {
            return Err(BridgeError::BridgeEventInUnrecognizedSuiPackage);
        }
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

    pub async fn get_token_transfer_action_onchain_status_until_success(
        &self,
        action: &BridgeAction,
    ) -> BridgeActionStatus {
        loop {
            let Ok(Ok(status)) = retry_with_max_elapsed_time!(
                self.inner.get_token_transfer_action_onchain_status(action),
                Duration::from_secs(30)
            ) else {
                // TODO: add metrics and fire alert
                error!("Failed to get action onchain status for: {:?}", action);
                continue;
            };
            return status;
        }
    }

    pub async fn get_gas_data_panic_if_not_gas(
        &self,
        gas_object_id: ObjectID,
    ) -> (GasCoin, ObjectRef, Owner) {
        self.inner
            .get_gas_data_panic_if_not_gas(gas_object_id)
            .await
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

    async fn get_token_transfer_action_onchain_status(
        &self,
        action: &BridgeAction,
    ) -> Result<BridgeActionStatus, BridgeError>;

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

    // TODO: Add a test for this
    async fn get_bridge_committee(&self) -> Result<MoveTypeBridgeCommittee, Self::Error> {
        let object_id = *get_bridge_object_id();
        let bcs_bytes = self.read_api().get_move_object_bcs(object_id).await?;
        let bridge_dynamic_field: BridgeInnerDynamicField = bcs::from_bytes(&bcs_bytes)?;
        Ok(bridge_dynamic_field.value.committee)
    }

    async fn get_token_transfer_action_onchain_status(
        &self,
        action: &BridgeAction,
    ) -> Result<BridgeActionStatus, BridgeError> {
        match &action {
            BridgeAction::SuiToEthBridgeAction(_) | BridgeAction::EthToSuiBridgeAction(_) => (),
            _ => return Err(BridgeError::ActionIsNotTokenTransferAction),
        };
        let package_id = *get_bridge_package_id();
        let key = serde_json::json!(
            {
                // u64 is represented as string
                "bridge_seq_num": action.seq_number().to_string(),
                "message_type": action.action_type() as u8,
                "source_chain": action.chain_id() as u8,
            }
        );
        let status_object_id = match self
            .read_api()
            .get_dynamic_field_object(
                *get_bridge_record_id(),
                DynamicFieldName {
                    type_: TypeTag::from_str(&format!(
                        "{:?}::message::BridgeMessageKey",
                        package_id
                    ))
                    .unwrap(),
                    value: key.clone(),
                },
            )
            .await?
            .into_object()
        {
            Ok(object) => object.object_id,
            Err(SuiObjectResponseError::DynamicFieldNotFound { .. }) => {
                return Ok(BridgeActionStatus::RecordNotFound)
            }
            other => {
                return Err(BridgeError::Generic(format!(
                    "Can't get bridge action record dynamic field {:?}: {:?}",
                    key, other
                )))
            }
        };

        // get_dynamic_field_object does not return bcs, so we have to issue anothe query
        let bcs_bytes = self
            .read_api()
            .get_move_object_bcs(status_object_id)
            .await?;
        let status_object: BridgeRecordDyanmicField = bcs::from_bytes(&bcs_bytes)?;

        if status_object.value.value.claimed {
            return Ok(BridgeActionStatus::Claimed);
        }

        if status_object.value.value.verified_signatures.is_some() {
            return Ok(BridgeActionStatus::Approved);
        }

        return Ok(BridgeActionStatus::Pending);
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
        events::{EmittedSuiToEthTokenBridgeV1, MoveTokenBridgeEvent},
        sui_mock_client::SuiMockClient,
        test_utils::{
            bridge_token, get_test_sui_to_eth_bridge_action, mint_tokens, publish_bridge_package,
            transfer_treasury_cap,
        },
        types::{BridgeActionType, BridgeChainId, SuiToEthBridgeAction, TokenId},
    };
    use ethers::{
        abi::Token,
        types::{
            Address as EthAddress, Block, BlockNumber, Filter, FilterBlockOption, Log,
            ValueOrArray, U64,
        },
    };
    use move_core_types::account_address::AccountAddress;
    use prometheus::Registry;
    use std::{collections::HashSet, str::FromStr};
    use test_cluster::TestClusterBuilder;

    use super::*;
    use crate::events::{init_all_struct_tags, SuiToEthTokenBridgeV1};

    #[tokio::test]
    async fn get_bridge_action_by_tx_digest_and_event_idx_maybe() {
        // Note: for random events generated in this test, we only care about
        // tx_digest and event_seq, so it's ok that package and module does
        // not match the query parameters.
        telemetry_subscribers::init_for_testing();
        let mock_client = SuiMockClient::default();
        let sui_client = SuiClient::new_for_testing(mock_client.clone());
        let tx_digest = TransactionDigest::random();

        // Ensure all struct tags are inited
        init_all_struct_tags();

        let sanitized_event_1 = EmittedSuiToEthTokenBridgeV1 {
            nonce: 1,
            sui_chain_id: BridgeChainId::SuiTestnet,
            sui_address: SuiAddress::random_for_testing_only(),
            eth_chain_id: BridgeChainId::EthSepolia,
            eth_address: Address::random(),
            token_id: TokenId::Sui,
            amount: 100,
        };
        let emitted_event_1 = MoveTokenBridgeEvent {
            message_type: BridgeActionType::TokenTransfer as u8,
            seq_num: sanitized_event_1.nonce,
            source_chain: sanitized_event_1.sui_chain_id as u8,
            sender_address: sanitized_event_1.sui_address.to_vec(),
            target_chain: sanitized_event_1.eth_chain_id as u8,
            target_address: sanitized_event_1.eth_address.as_bytes().to_vec(),
            token_type: sanitized_event_1.token_id as u8,
            amount: sanitized_event_1.amount,
        };

        // TODO: remove once we don't rely on env var to get package id
        std::env::set_var("BRIDGE_PACKAGE_ID", "0x0b");

        let mut sui_event_1 = SuiEvent::random_for_testing();
        sui_event_1.type_ = SuiToEthTokenBridgeV1.get().unwrap().clone();
        sui_event_1.bcs = bcs::to_bytes(&emitted_event_1).unwrap();

        #[derive(Serialize, Deserialize)]
        struct RandomStruct {};

        let event_2: RandomStruct = RandomStruct {};
        // undeclared struct tag
        let mut sui_event_2 = SuiEvent::random_for_testing();
        sui_event_2.type_ = SuiToEthTokenBridgeV1.get().unwrap().clone();
        sui_event_2.type_.module = Identifier::from_str("unrecognized_module").unwrap();
        sui_event_2.bcs = bcs::to_bytes(&event_2).unwrap();

        // Event 3 is defined in non-bridge package
        let mut sui_event_3 = sui_event_1.clone();
        sui_event_3.type_.address = AccountAddress::random();

        mock_client.add_events_by_tx_digest(
            tx_digest,
            vec![
                sui_event_1.clone(),
                sui_event_2.clone(),
                sui_event_1.clone(),
                sui_event_3.clone(),
            ],
        );
        let mut expected_action_1 = BridgeAction::SuiToEthBridgeAction(SuiToEthBridgeAction {
            sui_tx_digest: tx_digest,
            sui_tx_event_index: 0,
            sui_bridge_event: sanitized_event_1.clone(),
        });
        assert_eq!(
            sui_client
                .get_bridge_action_by_tx_digest_and_event_idx_maybe(&tx_digest, 0)
                .await
                .unwrap(),
            expected_action_1,
        );
        let mut expected_action_2 = BridgeAction::SuiToEthBridgeAction(SuiToEthBridgeAction {
            sui_tx_digest: tx_digest,
            sui_tx_event_index: 2,
            sui_bridge_event: sanitized_event_1.clone(),
        });
        assert_eq!(
            sui_client
                .get_bridge_action_by_tx_digest_and_event_idx_maybe(&tx_digest, 2)
                .await
                .unwrap(),
            expected_action_2,
        );
        assert!(matches!(
            sui_client
                .get_bridge_action_by_tx_digest_and_event_idx_maybe(&tx_digest, 1)
                .await
                .unwrap_err(),
            BridgeError::NoBridgeEventsInTxPosition
        ),);
        assert!(matches!(
            sui_client
                .get_bridge_action_by_tx_digest_and_event_idx_maybe(&tx_digest, 3)
                .await
                .unwrap_err(),
            BridgeError::BridgeEventInUnrecognizedSuiPackage
        ),);
        assert!(matches!(
            sui_client
                .get_bridge_action_by_tx_digest_and_event_idx_maybe(&tx_digest, 4)
                .await
                .unwrap_err(),
            BridgeError::NoBridgeEventsInTxPosition
        ),);

        // if the StructTag matches with unparsable bcs, it returns an error
        sui_event_2.type_ = SuiToEthTokenBridgeV1.get().unwrap().clone();
        mock_client.add_events_by_tx_digest(tx_digest, vec![sui_event_2]);
        sui_client
            .get_bridge_action_by_tx_digest_and_event_idx_maybe(&tx_digest, 2)
            .await
            .unwrap_err();
    }

    #[tokio::test]
    async fn test_get_action_onchain_status_for_sui_to_eth_transfer() {
        let mut test_cluster = TestClusterBuilder::new().build().await;
        let context = &mut test_cluster.wallet;
        let sender = context.active_address().unwrap();

        let treasury_caps = publish_bridge_package(context).await;
        let sui_client = SuiClient::new(&test_cluster.fullnode_handle.rpc_url)
            .await
            .unwrap();

        let action = get_test_sui_to_eth_bridge_action(None, None, None, None);

        let status = sui_client
            .inner
            .get_token_transfer_action_onchain_status(&action)
            .await
            .unwrap();
        assert_eq!(status, BridgeActionStatus::RecordNotFound);

        // mint 1000 USDC
        let amount = 1_000_000_000u64;
        let (treasury_cap_obj_ref, usdc_coin_obj_ref) = mint_tokens(
            context,
            treasury_caps[&TokenId::USDC],
            amount,
            TokenId::USDC,
        )
        .await;

        transfer_treasury_cap(context, treasury_cap_obj_ref, TokenId::USDC).await;

        let recv_address = EthAddress::random();
        let bridge_event =
            bridge_token(context, recv_address, usdc_coin_obj_ref, TokenId::USDC).await;
        assert_eq!(bridge_event.nonce, 0);
        assert_eq!(bridge_event.sui_chain_id, BridgeChainId::SuiLocalTest);
        assert_eq!(bridge_event.eth_chain_id, BridgeChainId::EthLocalTest);
        assert_eq!(bridge_event.eth_address, recv_address);
        assert_eq!(bridge_event.sui_address, sender);
        assert_eq!(bridge_event.token_id, TokenId::USDC);
        assert_eq!(bridge_event.amount, amount);

        let status = sui_client
            .inner
            .get_token_transfer_action_onchain_status(&action)
            .await
            .unwrap();
        assert_eq!(status, BridgeActionStatus::Pending);

        // TODO: run bridge committee and approve the action, then assert status is Approved
    }

    #[tokio::test]
    async fn test_get_action_onchain_status_for_eth_to_sui_transfer() {
        // TODO: init an eth -> sui transfer, run bridge committee, approve the action, then assert status is Approved/Claimed
    }
}
