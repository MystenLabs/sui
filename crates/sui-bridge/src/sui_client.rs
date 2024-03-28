// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// TODO remove when integrated
#![allow(unused)]

use anyhow::anyhow;
use async_trait::async_trait;
use axum::response::sse::Event;
use ethers::types::{Address, U256};
use fastcrypto::traits::KeyPair;
use fastcrypto::traits::ToFromBytes;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use std::str::from_utf8;
use std::str::FromStr;
use std::time::Duration;
use sui_json_rpc_api::BridgeReadApiClient;
use sui_json_rpc_types::DevInspectResults;
use sui_json_rpc_types::{EventFilter, Page, SuiData, SuiEvent};
use sui_json_rpc_types::{
    EventPage, SuiObjectDataOptions, SuiTransactionBlockResponse,
    SuiTransactionBlockResponseOptions,
};
use sui_sdk::{SuiClient as SuiSdkClient, SuiClientBuilder};
use sui_types::base_types::ObjectRef;
use sui_types::base_types::SequenceNumber;
use sui_types::bridge;
use sui_types::bridge::get_bridge;
use sui_types::bridge::BridgeCommitteeSummary;
use sui_types::bridge::BridgeInnerDynamicField;
use sui_types::bridge::BridgeRecordDyanmicField;
use sui_types::bridge::BridgeSummary;
use sui_types::bridge::MoveTypeBridgeCommittee;
use sui_types::bridge::MoveTypeCommitteeMember;
use sui_types::collection_types::LinkedTableNode;
use sui_types::crypto::get_key_pair;
use sui_types::dynamic_field::DynamicFieldName;
use sui_types::dynamic_field::Field;
use sui_types::error::SuiObjectResponseError;
use sui_types::error::UserInputError;
use sui_types::event;
use sui_types::gas_coin::GasCoin;
use sui_types::object::{Object, Owner};
use sui_types::transaction::Argument;
use sui_types::transaction::CallArg;
use sui_types::transaction::Command;
use sui_types::transaction::ObjectArg;
use sui_types::transaction::ProgrammableMoveCall;
use sui_types::transaction::ProgrammableTransaction;
use sui_types::transaction::Transaction;
use sui_types::transaction::TransactionKind;
use sui_types::TypeTag;
use sui_types::BRIDGE_PACKAGE_ID;
use sui_types::SUI_BRIDGE_OBJECT_ID;
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
use crate::types::BridgeActionStatus;
use crate::types::{BridgeAction, BridgeAuthority, BridgeCommittee};

pub struct SuiClient<P> {
    inner: P,
}

pub type SuiBridgeClient = SuiClient<SuiSdkClient>;

impl SuiBridgeClient {
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

    // TODO(bridge): cache this
    pub async fn get_mutable_bridge_object_arg(&self) -> BridgeResult<ObjectArg> {
        self.inner
            .get_mutable_bridge_object_arg()
            .await
            .map_err(|e| BridgeError::Generic(format!("Can't get mutable bridge object arg: {e}")))
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
        if event.type_.address.as_ref() != BRIDGE_PACKAGE_ID.as_ref() {
            return Err(BridgeError::BridgeEventInUnrecognizedSuiPackage);
        }
        let bridge_event = SuiBridgeEvent::try_from_sui_event(event)?
            .ok_or(BridgeError::NoBridgeEventsInTxPosition)?;

        bridge_event
            .try_into_bridge_action(*tx_digest, event_idx)
            .ok_or(BridgeError::BridgeEventNotActionable)
    }

    pub async fn get_bridge_summary(&self) -> BridgeResult<BridgeSummary> {
        self.inner
            .get_bridge_summary()
            .await
            .map_err(|e| BridgeError::InternalError(format!("Can't get bridge committee: {e}")))
    }

    // TODO: cache this
    pub async fn get_bridge_record_id(&self) -> BridgeResult<ObjectID> {
        self.inner
            .get_bridge_summary()
            .await
            .map_err(|e| BridgeError::InternalError(format!("Can't get bridge committee: {e}")))
            .map(|bridge_summary| bridge_summary.bridge_records_id)
    }

    pub async fn get_bridge_committee(&self) -> BridgeResult<BridgeCommittee> {
        let bridge_summary =
            self.inner.get_bridge_summary().await.map_err(|e| {
                BridgeError::InternalError(format!("Can't get bridge committee: {e}"))
            })?;
        let move_type_bridge_committee = bridge_summary.committee;

        let mut authorities = vec![];
        // TODO: move this to MoveTypeBridgeCommittee
        for (_, member) in move_type_bridge_committee.members {
            let MoveTypeCommitteeMember {
                sui_address,
                bridge_pubkey_bytes,
                voting_power,
                http_rest_url,
                blocklisted,
            } = member;
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
            let Ok(Ok(bridge_object_arg)) = retry_with_max_elapsed_time!(
                self.get_mutable_bridge_object_arg(),
                Duration::from_secs(30)
            ) else {
                // TODO: add metrics and fire alert
                error!("Failed to get bridge object arg");
                continue;
            };
            let Ok(Ok(status)) = retry_with_max_elapsed_time!(
                self.inner
                    .get_token_transfer_action_onchain_status(bridge_object_arg, action),
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

    async fn get_mutable_bridge_object_arg(&self) -> Result<ObjectArg, Self::Error>;

    async fn get_bridge_summary(&self) -> Result<BridgeSummary, Self::Error>;

    async fn execute_transaction_block_with_effects(
        &self,
        tx: Transaction,
    ) -> Result<SuiTransactionBlockResponse, BridgeError>;

    async fn get_token_transfer_action_onchain_status(
        &self,
        bridge_object_arg: ObjectArg,
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

    async fn get_mutable_bridge_object_arg(&self) -> Result<ObjectArg, Self::Error> {
        let initial_shared_version = self
            .http()
            .get_bridge_object_initial_shared_version()
            .await?;
        Ok(ObjectArg::SharedObject {
            id: SUI_BRIDGE_OBJECT_ID,
            initial_shared_version: SequenceNumber::from_u64(initial_shared_version),
            mutable: true,
        })
    }

    async fn get_bridge_summary(&self) -> Result<BridgeSummary, Self::Error> {
        self.http().get_latest_bridge().await.map_err(|e| e.into())
    }

    async fn get_token_transfer_action_onchain_status(
        &self,
        bridge_object_arg: ObjectArg,
        action: &BridgeAction,
    ) -> Result<BridgeActionStatus, BridgeError> {
        let pt = ProgrammableTransaction {
            inputs: vec![
                CallArg::Object(bridge_object_arg),
                CallArg::Pure(bcs::to_bytes(&(action.chain_id() as u8)).unwrap()),
                CallArg::Pure(bcs::to_bytes(&action.seq_number()).unwrap()),
            ],
            commands: vec![Command::MoveCall(Box::new(ProgrammableMoveCall {
                package: BRIDGE_PACKAGE_ID,
                module: Identifier::new("bridge").unwrap(),
                function: Identifier::new("get_token_transfer_action_status").unwrap(),
                type_arguments: vec![],
                arguments: vec![Argument::Input(0), Argument::Input(1), Argument::Input(2)],
            }))],
        };
        let kind = TransactionKind::programmable(pt.clone());
        let resp = self
            .read_api()
            .dev_inspect_transaction_block(SuiAddress::ZERO, kind, None, None, None)
            .await?;
        let DevInspectResults {
            results, effects, ..
        } = resp;
        let Some(results) = results else {
            return Err(BridgeError::Generic(format!(
                "Can't get token transfer action status (empty results). effects: {:?}",
                effects
            )));
        };
        let return_values = &results
            .first()
            .ok_or(BridgeError::Generic(format!(
                "Can't get token transfer action status, results: {:?}",
                results
            )))?
            .return_values;
        let (value_bytes, _type_tag) =
            return_values.first().ok_or(BridgeError::Generic(format!(
                "Can't get token transfer action status, results: {:?}",
                results
            )))?;
        let status = bcs::from_bytes::<u8>(value_bytes).map_err(|_e| {
            BridgeError::Generic(format!(
                "Can't parse token transfer action status as u8: {:?}",
                results
            ))
        })?;
        let status = BridgeActionStatus::try_from(status).map_err(|_e| {
            BridgeError::Generic(format!(
                "Can't parse token transfer action status as BridgeActionStatus: {:?}",
                results
            ))
        })?;

        return Ok(status);
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
    use crate::BRIDGE_ENABLE_PROTOCOL_VERSION;
    use crate::{
        events::{EmittedSuiToEthTokenBridgeV1, MoveTokenBridgeEvent},
        sui_mock_client::SuiMockClient,
        test_utils::{
            approve_action_with_validator_secrets, bridge_token, get_test_eth_to_sui_bridge_action,
            get_test_sui_to_eth_bridge_action,
        },
        types::{BridgeActionType, SuiToEthBridgeAction},
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
    use sui_json_rpc_types::SuiTransactionBlockEffectsAPI;
    use sui_sdk::wallet_context;
    use sui_types::bridge::{BridgeChainId, TokenId};
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

    // Test get_action_onchain_status.
    // Use validator secrets to bridge USDC from Ethereum initially.
    // TODO: we need an e2e test for this with published solidity contract and committee with BridgeNodes
    #[tokio::test]
    async fn test_get_action_onchain_status_for_sui_to_eth_transfer() {
        telemetry_subscribers::init_for_testing();
        let mut test_cluster: test_cluster::TestCluster = TestClusterBuilder::new()
            .with_protocol_version((BRIDGE_ENABLE_PROTOCOL_VERSION).into())
            .with_epoch_duration_ms(15000)
            .build_with_bridge()
            .await;

        let sui_client = SuiClient::new(&test_cluster.fullnode_handle.rpc_url)
            .await
            .unwrap();
        let bridge_authority_keys = test_cluster.bridge_authority_keys.take().unwrap();

        // Return once the bridge committee is initialized. Return immediately if it's already the case.
        // Otherwise wait until the next epoch. This call should be called after proper bridge committee setup,
        // such as `build_with_bridge`.
        // Note: We don't call `sui_client.get_bridge_committee` here because it will err if the committee
        // is not initialized during the construction of `BridgeCommittee`.
        let committee = sui_client.get_bridge_summary().await.unwrap().committee;
        if committee.members.is_empty() {
            test_cluster.wait_for_epoch(None).await;
        }
        let context = &mut test_cluster.wallet;
        let sender = context.active_address().unwrap();
        let summary = sui_client.inner.get_bridge_summary().await.unwrap();
        let usdc_amount = 5000000;
        let bridge_object_arg = sui_client.get_mutable_bridge_object_arg().await.unwrap();

        // 1. Create a Eth -> Sui Transfer (recipient is sender address), approve with validator secrets and assert its status to be Claimed
        let action = get_test_eth_to_sui_bridge_action(None, Some(usdc_amount), Some(sender));
        let usdc_object_ref = approve_action_with_validator_secrets(
            context,
            bridge_object_arg,
            action.clone(),
            &bridge_authority_keys,
            Some(sender),
        )
        .await
        .unwrap();

        let status = sui_client
            .inner
            .get_token_transfer_action_onchain_status(bridge_object_arg, &action)
            .await
            .unwrap();
        assert_eq!(status, BridgeActionStatus::Claimed);

        // 2. Create a Sui -> Eth Transfer, approve with validator secrets and assert its status to be Approved
        // We need to actually send tokens to bridge to initialize the record.
        let eth_recv_address = EthAddress::random();
        let bridge_event = bridge_token(
            context,
            eth_recv_address,
            usdc_object_ref,
            TokenId::USDC,
            bridge_object_arg,
        )
        .await;
        assert_eq!(bridge_event.nonce, 0);
        assert_eq!(bridge_event.sui_chain_id, BridgeChainId::SuiLocalTest);
        assert_eq!(bridge_event.eth_chain_id, BridgeChainId::EthLocalTest);
        assert_eq!(bridge_event.eth_address, eth_recv_address);
        assert_eq!(bridge_event.sui_address, sender);
        assert_eq!(bridge_event.token_id, TokenId::USDC);
        assert_eq!(bridge_event.amount, usdc_amount);

        let action = get_test_sui_to_eth_bridge_action(
            None,
            None,
            Some(bridge_event.nonce),
            Some(bridge_event.amount),
            Some(bridge_event.sui_address),
            Some(bridge_event.eth_address),
            Some(TokenId::USDC),
        );
        let status = sui_client
            .inner
            .get_token_transfer_action_onchain_status(bridge_object_arg, &action)
            .await
            .unwrap();
        // At this point, the record is created and the status is Pending
        assert_eq!(status, BridgeActionStatus::Pending);

        // Approve it and assert its status to be Approved
        approve_action_with_validator_secrets(
            context,
            bridge_object_arg,
            action.clone(),
            &bridge_authority_keys,
            None,
        )
        .await;

        let status = sui_client
            .inner
            .get_token_transfer_action_onchain_status(bridge_object_arg, &action)
            .await
            .unwrap();
        assert_eq!(status, BridgeActionStatus::Approved);

        // 3. Create a random action and assert its status as NotFound
        let action =
            get_test_sui_to_eth_bridge_action(None, None, Some(100), None, None, None, None);
        let status = sui_client
            .inner
            .get_token_transfer_action_onchain_status(bridge_object_arg, &action)
            .await
            .unwrap();
        assert_eq!(status, BridgeActionStatus::NotFound);
    }
}
