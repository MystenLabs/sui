// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::collections::HashMap;
use std::ops::Range;
use std::str::FromStr;

use fastcrypto::traits::EncodeDecodeBase64;
use move_core_types::annotated_value::MoveStructLayout;
use move_core_types::identifier::Identifier;
use move_core_types::language_storage::ModuleId;
use move_core_types::language_storage::{StructTag, TypeTag};
use move_core_types::resolver::ModuleResolver;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use serde_json::json;

use sui_json::SuiJsonValue;
use sui_json_rpc::error::Error;
use sui_json_rpc_types::BcsEvent;
use sui_json_rpc_types::DevInspectArgs;
use sui_json_rpc_types::{
    Balance, Checkpoint, CheckpointId, CheckpointPage, Coin, CoinPage, DelegatedStake,
    DevInspectResults, DynamicFieldPage, EventFilter, EventPage, MoveCallParams,
    MoveFunctionArgType, ObjectChange, ObjectValueKind::ByImmutableReference,
    ObjectValueKind::ByMutableReference, ObjectValueKind::ByValue, ObjectsPage, OwnedObjectRef,
    Page, ProtocolConfigResponse, RPCTransactionRequestParams, Stake, StakeStatus, SuiCoinMetadata,
    SuiCommittee, SuiData, SuiEvent, SuiExecutionStatus, SuiGetPastObjectRequest, SuiMoveAbility,
    SuiMoveAbilitySet, SuiMoveNormalizedFunction, SuiMoveNormalizedModule, SuiMoveNormalizedStruct,
    SuiMoveNormalizedType, SuiMoveVisibility, SuiObjectData, SuiObjectDataFilter,
    SuiObjectDataOptions, SuiObjectRef, SuiObjectResponse, SuiObjectResponseQuery, SuiParsedData,
    SuiPastObjectResponse, SuiTransactionBlock, SuiTransactionBlockData,
    SuiTransactionBlockEffects, SuiTransactionBlockEffectsV1, SuiTransactionBlockEvents,
    SuiTransactionBlockResponse, SuiTransactionBlockResponseOptions,
    SuiTransactionBlockResponseQuery, TransactionBlockBytes, TransactionBlocksPage,
    TransactionFilter, TransferObjectParams,
};
use sui_json_rpc_types::{SuiTypeTag, ValidatorApy, ValidatorApys};
use sui_open_rpc::ExamplePairing;
use sui_protocol_config::Chain;
use sui_protocol_config::ProtocolConfig;
use sui_types::balance::Supply;
use sui_types::base_types::random_object_ref;
use sui_types::base_types::{
    MoveObjectType, ObjectDigest, ObjectID, ObjectType, SequenceNumber, SuiAddress,
    TransactionDigest,
};
use sui_types::committee::Committee;
use sui_types::crypto::{get_key_pair_from_rng, AccountKeyPair, AggregateAuthoritySignature};
use sui_types::digests::TransactionEventsDigest;
use sui_types::dynamic_field::{DynamicFieldInfo, DynamicFieldName, DynamicFieldType};
use sui_types::event::EventID;
use sui_types::gas::GasCostSummary;
use sui_types::gas_coin::GasCoin;
use sui_types::messages_checkpoint::CheckpointDigest;
use sui_types::object::MoveObject;
use sui_types::object::Owner;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::quorum_driver_types::ExecuteTransactionRequestType;
use sui_types::signature::GenericSignature;
use sui_types::transaction::ObjectArg;
use sui_types::transaction::TEST_ONLY_GAS_UNIT_FOR_TRANSFER;
use sui_types::transaction::{CallArg, TransactionData};
use sui_types::utils::to_sender_signed_transaction;
use sui_types::{parse_sui_struct_tag, SUI_FRAMEWORK_PACKAGE_ID};

struct Examples {
    function_name: String,
    examples: Vec<ExamplePairing>,
}

impl Examples {
    fn new(name: &str, examples: Vec<ExamplePairing>) -> Self {
        Self {
            function_name: name.to_string(),
            examples,
        }
    }
}

pub struct RpcExampleProvider {
    rng: StdRng,
}

impl RpcExampleProvider {
    pub fn new() -> Self {
        Self {
            rng: StdRng::from_seed([0; 32]),
        }
    }

    pub fn examples(&mut self) -> BTreeMap<String, Vec<ExamplePairing>> {
        [
            self.batch_transaction_examples(),
            self.get_object_example(),
            self.get_past_object_example(),
            self.get_owned_objects(),
            self.get_total_transaction_blocks(),
            self.get_transaction_block(),
            self.query_transaction_blocks(),
            self.get_events(),
            self.execute_transaction_example(),
            self.dry_run_transaction_block(),
            self.dev_inspect_transaction_block(),
            self.get_checkpoint_example(),
            self.get_checkpoints(),
            self.sui_get_committee_info(),
            self.sui_get_reference_gas_price(),
            self.suix_get_all_balances(),
            self.suix_get_all_coins(),
            self.suix_get_balance(),
            self.suix_get_coin_metadata(),
            self.sui_get_latest_checkpoint_sequence_number(),
            self.suix_get_coins(),
            self.suix_get_total_supply(),
            self.suix_get_dynamic_fields(),
            self.suix_get_dynamic_field_object(),
            self.suix_get_owned_objects(),
            self.sui_get_move_function_arg_types(),
            self.sui_get_normalized_move_function(),
            self.sui_get_normalized_move_module(),
            self.sui_get_normalized_move_modules_by_package(),
            self.sui_get_normalized_move_struct(),
            self.multi_get_objects_example(),
            self.multi_get_transaction_blocks(),
            self.suix_get_validators_apy(),
            self.suix_get_dynamic_fields(),
            self.suix_get_dynamic_field_object(),
            self.suix_get_owned_objects(),
            self.suix_query_events(),
            self.suix_get_latest_sui_system_state(),
            self.get_protocol_config(),
            self.sui_get_chain_identifier(),
            self.suix_get_stakes(),
            self.suix_get_stakes_by_ids(),
            self.suix_resolve_name_service_address(),
            self.suix_resolve_name_service_names(),
            self.sui_try_multi_get_past_objects(),
        ]
        .into_iter()
        .map(|example| (example.function_name, example.examples))
        .collect()
    }

    fn batch_transaction_examples(&mut self) -> Examples {
        let signer = SuiAddress::from(ObjectID::new(self.rng.gen()));
        let recipient = SuiAddress::from(ObjectID::new(self.rng.gen()));
        let gas_id = ObjectID::new(self.rng.gen());
        let object_id = ObjectID::new(self.rng.gen());
        let coin_ref = random_object_ref();
        let random_amount: u64 = 10;

        let tx_params = vec![
            RPCTransactionRequestParams::MoveCallRequestParams(MoveCallParams {
                package_object_id: SUI_FRAMEWORK_PACKAGE_ID,
                module: "pay".to_string(),
                function: "split".to_string(),
                type_arguments: vec![SuiTypeTag::new("0x2::sui::SUI".to_string())],
                arguments: vec![
                    SuiJsonValue::new(json!(coin_ref.0)).unwrap(),
                    SuiJsonValue::new(json!(random_amount)).unwrap(),
                ],
            }),
            RPCTransactionRequestParams::TransferObjectRequestParams(TransferObjectParams {
                recipient,
                object_id,
            }),
        ];

        let pt = {
            let mut builder = ProgrammableTransactionBuilder::new();
            builder
                .move_call(
                    SUI_FRAMEWORK_PACKAGE_ID,
                    Identifier::from_str("pay").unwrap(),
                    Identifier::from_str("split").unwrap(),
                    vec![],
                    vec![
                        CallArg::Object(ObjectArg::ImmOrOwnedObject(coin_ref)),
                        CallArg::Pure(bcs::to_bytes(&random_amount).unwrap()),
                    ],
                )
                .unwrap();
            builder
                .transfer_object(
                    recipient,
                    (
                        object_id,
                        SequenceNumber::from_u64(1),
                        ObjectDigest::new(self.rng.gen()),
                    ),
                )
                .unwrap();
            builder.finish()
        };
        let gas_price = 10;
        let data = TransactionData::new_programmable(
            signer,
            vec![(
                gas_id,
                SequenceNumber::from_u64(1),
                ObjectDigest::new(self.rng.gen()),
            )],
            pt,
            TEST_ONLY_GAS_UNIT_FOR_TRANSFER * gas_price,
            gas_price,
        );

        let result = TransactionBlockBytes::from_data(data).unwrap();

        Examples::new(
            "sui_batchTransaction",
            vec![ExamplePairing::new(
                "Creates unsigned batch transaction data.",
                vec![
                    ("signer", json!(signer)),
                    ("single_transaction_params", json!(tx_params)),
                    ("gas", json!(gas_id)),
                    ("gas_budget", json!(1000)),
                    ("txn_builder_mode", json!("Commit")),
                ],
                json!(result),
            )],
        )
    }

    fn execute_transaction_example(&mut self) -> Examples {
        let (data, signatures, _, _, result) = self.get_transfer_data_response();
        let tx_bytes = TransactionBlockBytes::from_data(data).unwrap();

        Examples::new(
            "sui_executeTransactionBlock",
            vec![ExamplePairing::new(
                "Executes a transaction with serialized signatures.",
                vec![
                    ("tx_bytes", json!(tx_bytes.tx_bytes)),
                    (
                        "signatures",
                        json!(signatures
                            .into_iter()
                            .map(|sig| sig.encode_base64())
                            .collect::<Vec<_>>()),
                    ),
                    (
                        "options",
                        json!(SuiTransactionBlockResponseOptions::full_content()),
                    ),
                    (
                        "request_type",
                        json!(ExecuteTransactionRequestType::WaitForLocalExecution),
                    ),
                ],
                json!(result),
            )],
        )
    }

    fn dry_run_transaction_block(&mut self) -> Examples {
        let (data, _, _, _, result) = self.get_transfer_data_response();
        let tx_bytes = TransactionBlockBytes::from_data(data).unwrap();

        Examples::new(
            "sui_dryRunTransactionBlock",
            vec![ExamplePairing::new(
                "Dry runs a transaction block to get back estimated gas fees and other potential effects.",
                vec![
                    ("tx_bytes", json!(tx_bytes.tx_bytes)),
                ],
                json!(result),
            )],
        )
    }

    fn dev_inspect_transaction_block(&mut self) -> Examples {
        let (data, _, _, _, result) = self.get_transfer_data_response();
        let tx_bytes = TransactionBlockBytes::from_data(data).unwrap();

        let dev_inspect_results = DevInspectResults {
            effects: result.effects.unwrap(),
            events: SuiTransactionBlockEvents { data: vec![] },
            results: None,
            error: None,
            raw_txn_data: vec![],
            raw_effects: vec![],
        };

        Examples::new(
            "sui_devInspectTransactionBlock",
            vec![ExamplePairing::new(
                "Runs the transaction in dev-inspect mode. Which allows for nearly any transaction (or Move call) with any arguments. Detailed results are provided, including both the transaction effects and any return values.",
                vec![
                    ("sender_address", json!(SuiAddress::from(ObjectID::new(self.rng.gen())))),
                    ("tx_bytes", json!(tx_bytes.tx_bytes)),
                    ("gas_price", json!(1000)),
                    ("epoch", json!(8888)),
                    ("additional_args", json!(None::<DevInspectArgs>)),
                ],
                json!(dev_inspect_results),
            )],
        )
    }

    fn multi_get_objects_example(&mut self) -> Examples {
        let objects = self.get_object_responses(5);
        let object_ids = objects
            .iter()
            .map(|o| o.object_id().unwrap())
            .collect::<Vec<_>>();
        Examples::new(
            "sui_multiGetObjects",
            vec![ExamplePairing::new(
                "Gets objects by IDs.",
                vec![
                    ("object_ids", json!(object_ids)),
                    ("options", json!(SuiObjectDataOptions::full_content())),
                ],
                json!(objects),
            )],
        )
    }

    fn get_object_responses(&mut self, object_count: usize) -> Vec<SuiObjectResponse> {
        (0..object_count)
            .map(|_| {
                let object_id = ObjectID::new(self.rng.gen());
                let coin = GasCoin::new(object_id, 100000000);

                SuiObjectResponse::new_with_data(SuiObjectData {
                    content: Some(
                        SuiParsedData::try_from_object(
                            coin.to_object(SequenceNumber::from_u64(1)),
                            GasCoin::layout(),
                        )
                        .unwrap(),
                    ),
                    owner: Some(Owner::AddressOwner(SuiAddress::from(ObjectID::new(
                        self.rng.gen(),
                    )))),
                    previous_transaction: Some(TransactionDigest::new(self.rng.gen())),
                    storage_rebate: Some(100),
                    object_id,
                    version: SequenceNumber::from_u64(1),
                    digest: ObjectDigest::new(self.rng.gen()),
                    type_: Some(ObjectType::Struct(MoveObjectType::gas_coin())),
                    bcs: None,
                    display: None,
                })
            })
            .collect()
    }

    fn get_object_example(&mut self) -> Examples {
        let result = self.get_object_responses(1).pop().unwrap();
        Examples::new(
            "sui_getObject",
            vec![ExamplePairing::new(
                "Gets Object data for the ID in the request.",
                vec![
                    ("object_id", json!(result.object_id().unwrap())),
                    ("options", json!(SuiObjectDataOptions::full_content())),
                ],
                json!(result),
            )],
        )
    }

    fn get_past_object_example(&mut self) -> Examples {
        let object_id = ObjectID::new(self.rng.gen());

        let coin = GasCoin::new(object_id, 10000);

        let result = SuiPastObjectResponse::VersionFound(SuiObjectData {
            content: Some(
                SuiParsedData::try_from_object(
                    coin.to_object(SequenceNumber::from_u64(1)),
                    GasCoin::layout(),
                )
                .unwrap(),
            ),
            owner: Some(Owner::AddressOwner(SuiAddress::from(ObjectID::new(
                self.rng.gen(),
            )))),
            previous_transaction: Some(TransactionDigest::new(self.rng.gen())),
            storage_rebate: Some(100),
            object_id,
            version: SequenceNumber::from_u64(4),
            digest: ObjectDigest::new(self.rng.gen()),
            type_: Some(ObjectType::Struct(MoveObjectType::gas_coin())),
            bcs: None,
            display: None,
        });

        Examples::new(
            "sui_tryGetPastObject",
            vec![ExamplePairing::new(
                "Gets Past Object data.",
                vec![
                    ("object_id", json!(object_id)),
                    ("version", json!(4)),
                    ("options", json!(SuiObjectDataOptions::full_content())),
                ],
                json!(result),
            )],
        )
    }

    fn get_checkpoint_example(&mut self) -> Examples {
        let result = Checkpoint {
            epoch: 5000,
            sequence_number: 1000,
            digest: CheckpointDigest::new(self.rng.gen()),
            network_total_transactions: 792385,
            previous_digest: Some(CheckpointDigest::new(self.rng.gen())),
            epoch_rolling_gas_cost_summary: Default::default(),
            timestamp_ms: 1676911928,
            end_of_epoch_data: None,
            transactions: vec![TransactionDigest::new(self.rng.gen())],
            checkpoint_commitments: vec![],
            validator_signature: AggregateAuthoritySignature::default(),
        };

        Examples::new(
            "sui_getCheckpoint",
            vec![ExamplePairing::new(
                "Gets checkpoint information for the checkpoint ID in the request.",
                vec![("id", json!(CheckpointId::SequenceNumber(1000)))],
                json!(result),
            )],
        )
    }

    fn get_checkpoints(&mut self) -> Examples {
        let limit = 4;
        let descending_order = false;
        let seq = 1004;
        let page = (0..4)
            .map(|idx| Checkpoint {
                epoch: 5000,
                sequence_number: seq + 1 + idx,
                digest: CheckpointDigest::new(self.rng.gen()),
                network_total_transactions: 792385,
                previous_digest: Some(CheckpointDigest::new(self.rng.gen())),
                epoch_rolling_gas_cost_summary: Default::default(),
                timestamp_ms: 1676911928,
                end_of_epoch_data: None,
                transactions: vec![TransactionDigest::new(self.rng.gen())],
                checkpoint_commitments: vec![],
                validator_signature: AggregateAuthoritySignature::default(),
            })
            .collect::<Vec<_>>();
        let pagelen = page.len() as u64;
        let result = CheckpointPage {
            data: page,
            next_cursor: Some((seq + pagelen).into()),
            has_next_page: true,
        };

        Examples::new(
            "sui_getCheckpoints",
            vec![ExamplePairing::new(
                "Gets a paginated list in descending order of all checkpoints starting at the provided cursor. Each page of results has a maximum number of checkpoints set by the provided limit.",
                vec![(
                         "cursor", json!(seq.to_string()),
                     ),
                     (
                         "limit", json!(limit),
                     ),
                     (
                         "descending_order",
                         json!(descending_order),
                     ),
                ],
                json!(result),
            )],
        )
    }

    fn get_owned_objects(&mut self) -> Examples {
        let owner = SuiAddress::from(ObjectID::new(self.rng.gen()));
        let result = (0..4)
            .map(|_| SuiObjectData {
                object_id: ObjectID::new(self.rng.gen()),
                version: Default::default(),
                digest: ObjectDigest::new(self.rng.gen()),
                type_: Some(ObjectType::Struct(MoveObjectType::gas_coin())),
                owner: Some(Owner::AddressOwner(owner)),
                previous_transaction: Some(TransactionDigest::new(self.rng.gen())),
                storage_rebate: None,
                display: None,
                content: None,
                bcs: None,
            })
            .collect::<Vec<_>>();

        Examples::new(
            "sui_getOwnedObjects",
            vec![ExamplePairing::new(
                "Gets objects owned by the address in the request.",
                vec![
                    ("address", json!(owner)),
                    (
                        "query",
                        json!(SuiObjectResponseQuery {
                            filter: Some(SuiObjectDataFilter::StructType(
                                StructTag::from_str("0x2::coin::Coin<0x2::sui::SUI>").unwrap()
                            )),
                            options: Some(
                                SuiObjectDataOptions::new()
                                    .with_type()
                                    .with_owner()
                                    .with_previous_transaction()
                            )
                        }),
                    ),
                    ("cursor", json!(ObjectID::new(self.rng.gen()))),
                    ("limit", json!(100)),
                    ("at_checkpoint", json!(None::<CheckpointId>)),
                ],
                json!(result),
            )],
        )
    }

    fn get_total_transaction_blocks(&mut self) -> Examples {
        Examples::new(
            "sui_getTotalTransactionBlocks",
            vec![ExamplePairing::new(
                "Gets total number of transactions on the network.",
                vec![],
                json!("2451485"),
            )],
        )
    }

    fn get_transaction_block(&mut self) -> Examples {
        let (_, _, _, _, result) = self.get_transfer_data_response();
        Examples::new(
            "sui_getTransactionBlock",
            vec![ExamplePairing::new(
                "Returns the transaction response object for specified transaction digest.",
                vec![
                    ("digest", json!(result.digest)),
                    (
                        "options",
                        json!(SuiTransactionBlockResponseOptions::new()
                            .with_input()
                            .with_effects()
                            .with_events()),
                    ),
                ],
                json!(result),
            )],
        )
    }

    fn query_transaction_blocks(&mut self) -> Examples {
        let mut data = self.get_transaction_digests(5..9);
        let has_next_page = data.len() > (9 - 5);
        data.truncate(9 - 5);
        let next_cursor = data.last().cloned();
        let data = data
            .into_iter()
            .map(SuiTransactionBlockResponse::new)
            .collect();

        let result = TransactionBlocksPage {
            data,
            next_cursor,
            has_next_page,
        };
        Examples::new(
            "suix_queryTransactionBlocks",
            vec![ExamplePairing::new(
                "Returns the transaction digest for specified query criteria.",
                vec![
                    (
                        "query",
                        json!(SuiTransactionBlockResponseQuery {
                            filter: Some(TransactionFilter::InputObject(ObjectID::new(
                                self.rng.gen()
                            ))),
                            options: None,
                        }),
                    ),
                    ("cursor", json!(TransactionDigest::new(self.rng.gen()))),
                    ("limit", json!(100)),
                    ("descending_order", json!(false)),
                ],
                json!(result),
            )],
        )
    }

    fn multi_get_transaction_blocks(&mut self) -> Examples {
        let data = (0..3)
            .map(|_| self.get_transfer_data_response().4)
            .collect::<Vec<_>>();
        let digests = data.iter().map(|x| x.digest).collect::<Vec<_>>();
        Examples::new(
            "sui_multiGetTransactionBlocks",
            vec![ExamplePairing::new(
                "Returns the transaction data for specified digest.",
                vec![
                    ("digests", json!(digests)),
                    (
                        "options",
                        json!(SuiTransactionBlockResponseOptions::new()
                            .with_input()
                            .with_effects()
                            .with_events()),
                    ),
                ],
                json!(data),
            )],
        )
    }

    fn get_transaction_digests(&mut self, range: Range<u64>) -> Vec<TransactionDigest> {
        range
            .into_iter()
            .map(|_| TransactionDigest::new(self.rng.gen()))
            .collect()
    }

    fn get_event_ids(&mut self, range: Range<u64>) -> Vec<EventID> {
        range
            .into_iter()
            .map(|_| EventID {
                tx_digest: TransactionDigest::new(self.rng.gen()),
                event_seq: 1,
            })
            .collect()
    }

    fn get_protocol_config(&mut self) -> Examples {
        let version = Some(6);
        Examples::new(
            "sui_getProtocolConfig",
            vec![ExamplePairing::new(
                "Returns the protocol config for the given protocol version. If none is specified, the node uses the version of the latest epoch it has processed",
                vec![
                    ("version", json!(version)),
                ],
                json!(Self::get_protocol_config_impl(version)),
            )],
        )
    }

    fn get_protocol_config_impl(version: Option<u64>) -> ProtocolConfigResponse {
        ProtocolConfigResponse::from(
            version
                .map(|v| {
                    ProtocolConfig::get_for_version_if_supported(v.into(), Chain::Unknown)
                        .unwrap_or(ProtocolConfig::get_for_min_version())
                })
                .unwrap_or(ProtocolConfig::get_for_min_version()),
        )
    }

    fn get_transfer_data_response(
        &mut self,
    ) -> (
        TransactionData,
        Vec<GenericSignature>,
        SuiAddress,
        ObjectID,
        SuiTransactionBlockResponse,
    ) {
        let (signer, kp): (_, AccountKeyPair) = get_key_pair_from_rng(&mut self.rng);
        let recipient = SuiAddress::from(ObjectID::new(self.rng.gen()));
        let obj_id = ObjectID::new(self.rng.gen());
        let gas_ref = (
            ObjectID::new(self.rng.gen()),
            SequenceNumber::from_u64(2),
            ObjectDigest::new(self.rng.gen()),
        );
        let object_ref = (
            obj_id,
            SequenceNumber::from_u64(2),
            ObjectDigest::new(self.rng.gen()),
        );

        let data = TransactionData::new_transfer(
            recipient,
            object_ref,
            signer,
            gas_ref,
            TEST_ONLY_GAS_UNIT_FOR_TRANSFER * 10,
            10,
        );
        let data1 = data.clone();
        let data2 = data.clone();

        let tx = to_sender_signed_transaction(data, &kp);
        let signatures = tx.data().tx_signatures().to_vec();
        let raw_transaction = bcs::to_bytes(tx.data()).unwrap();

        let tx_digest = tx.digest();
        let object_change = ObjectChange::Transferred {
            sender: signer,
            recipient: Owner::AddressOwner(recipient),
            object_type: parse_sui_struct_tag("0x2::example::Object").unwrap(),
            object_id: object_ref.0,
            version: object_ref.1,
            digest: ObjectDigest::new(self.rng.gen()),
        };
        struct NoOpsModuleResolver;
        impl ModuleResolver for NoOpsModuleResolver {
            type Error = Error;
            fn get_module(&self, _id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
                Ok(None)
            }
        }
        let result = SuiTransactionBlockResponse {
            digest: *tx_digest,
            effects: Some(SuiTransactionBlockEffects::V1(
                SuiTransactionBlockEffectsV1 {
                    status: SuiExecutionStatus::Success,
                    executed_epoch: 0,
                    modified_at_versions: vec![],
                    gas_used: GasCostSummary {
                        computation_cost: 100,
                        storage_cost: 100,
                        storage_rebate: 10,
                        non_refundable_storage_fee: 0,
                    },
                    shared_objects: vec![],
                    transaction_digest: TransactionDigest::new(self.rng.gen()),
                    created: vec![],
                    mutated: vec![
                        OwnedObjectRef {
                            owner: Owner::AddressOwner(signer),
                            reference: gas_ref.into(),
                        },
                        OwnedObjectRef {
                            owner: Owner::AddressOwner(recipient),
                            reference: object_ref.into(),
                        },
                    ],
                    unwrapped: vec![],
                    deleted: vec![],
                    unwrapped_then_deleted: vec![],
                    wrapped: vec![],
                    gas_object: OwnedObjectRef {
                        owner: Owner::ObjectOwner(signer),
                        reference: SuiObjectRef::from(gas_ref),
                    },
                    events_digest: Some(TransactionEventsDigest::new(self.rng.gen())),
                    dependencies: vec![],
                },
            )),
            events: None,
            object_changes: Some(vec![object_change]),
            balance_changes: None,
            timestamp_ms: None,
            transaction: Some(SuiTransactionBlock {
                data: SuiTransactionBlockData::try_from(data1, &&mut NoOpsModuleResolver).unwrap(),
                tx_signatures: signatures.clone(),
            }),
            raw_transaction,
            confirmed_local_execution: None,
            checkpoint: None,
            errors: vec![],
            raw_effects: vec![],
        };

        (data2, signatures, recipient, obj_id, result)
    }

    fn get_events(&mut self) -> Examples {
        let tx_dig =
            TransactionDigest::from_str("11a72GCQ5hGNpWGh2QhQkkusTEGS6EDqifJqxr7nSYX").unwrap();
        let event = SuiEvent {
            id: EventID {
                tx_digest: tx_dig,
                event_seq: 0,
            },
            package_id: ObjectID::new(self.rng.gen()),
            transaction_module: Identifier::from_str("test_module").unwrap(),
            sender: SuiAddress::from(ObjectID::new(self.rng.gen())),
            type_: parse_sui_struct_tag("0x9::test::TestEvent").unwrap(),
            parsed_json: json!({"test": "example value"}),
            bcs: BcsEvent::new(vec![]),
            timestamp_ms: None,
        };

        let page = EventPage {
            data: vec![event],
            next_cursor: Some((tx_dig, 5).into()),
            has_next_page: false,
        };
        Examples::new(
            "sui_getEvents",
            vec![ExamplePairing::new(
                "Returns the events the transaction in the request emits.",
                vec![("transaction_digest", json!(tx_dig))],
                json!(page),
            )],
        )
    }

    fn sui_get_committee_info(&mut self) -> Examples {
        let epoch = 5000;
        let committee = json!(Committee::new_simple_test_committee_of_size(4));
        let vals = json!(committee[0]["voting_rights"]);
        let suicomm = SuiCommittee {
            epoch,
            validators: serde_json::from_value(vals).unwrap(),
        };

        Examples::new(
            "suix_getCommitteeInfo",
            vec![ExamplePairing::new(
                "Gets committee information for epoch 5000.",
                vec![("epoch", json!(epoch.to_string()))],
                json!(suicomm),
            )],
        )
    }

    fn sui_get_reference_gas_price(&mut self) -> Examples {
        let result = 1000;
        Examples::new(
            "suix_getReferenceGasPrice",
            vec![ExamplePairing::new(
                "Gets reference gas price information for the network.",
                vec![],
                json!(result),
            )],
        )
    }

    fn suix_get_all_balances(&mut self) -> Examples {
        let address = SuiAddress::from(ObjectID::new(self.rng.gen()));

        let result = Balance {
            coin_type: "0x2::sui::SUI".to_string(),
            coin_object_count: 15,
            total_balance: 3000000000,
            locked_balance: HashMap::new(),
        };
        Examples::new(
            "suix_getAllBalances",
            vec![ExamplePairing::new(
                "Gets all balances for the address in the request.",
                vec![("owner", json!(address))],
                json!(vec![result]),
            )],
        )
    }

    fn suix_get_all_coins(&mut self) -> Examples {
        let limit = 3;
        let owner = SuiAddress::from(ObjectID::new(self.rng.gen()));
        let cursor = ObjectID::new(self.rng.gen());
        let coins = (0..3)
            .map(|_| Coin {
                coin_type: "0x2::sui::SUI".to_string(),
                coin_object_id: ObjectID::new(self.rng.gen()),
                version: SequenceNumber::from_u64(103626),
                digest: ObjectDigest::new(self.rng.gen()),
                balance: 200000000,
                //locked_until_epoch: None,
                previous_transaction: TransactionDigest::new(self.rng.gen()),
            })
            .collect::<Vec<_>>();
        let page = CoinPage {
            data: coins,
            next_cursor: Some("abcd".to_string()),
            has_next_page: true,
        };

        Examples::new(
            "suix_getAllCoins",
            vec![ExamplePairing::new(
                "Gets all coins for the address in the request body. Begin listing the coins that are after the provided `cursor` value and return only the `limit` amount of results per page.",
                vec![
                    ("owner", json!(owner)),
                    ("cursor", json!(cursor)),
                    ("limit", json!(limit)),
                ],
                json!(page),
            )],
        )
    }

    fn suix_get_balance(&mut self) -> Examples {
        let owner = SuiAddress::from(ObjectID::new(self.rng.gen()));
        let coin_type = "0x168da5bf1f48dafc111b0a488fa454aca95e0b5e::usdc::USDC".to_string();
        let result = Balance {
            coin_type: coin_type.clone(),
            coin_object_count: 15,
            total_balance: 15,
            locked_balance: HashMap::new(),
        };

        Examples::new(
            "suix_getBalance",
            vec![ExamplePairing::new(
                "Gets the balance of the specified type of coin for the address in the request.",
                vec![("owner", json!(owner)), ("coin_type", json!(coin_type))],
                json!(result),
            )],
        )
    }

    fn suix_get_coin_metadata(&mut self) -> Examples {
        let result = SuiCoinMetadata {
            decimals: 9,
            name: "Usdc".to_string(),
            symbol: "USDC".to_string(),
            description: "Stable coin.".to_string(),
            icon_url: None,
            id: Some(ObjectID::new(self.rng.gen())),
        };

        Examples::new(
            "suix_getCoinMetadata",
            vec![ExamplePairing::new(
                "Gets the metadata for the coin type in the request.",
                vec![(
                    "coin_type",
                    json!("0x168da5bf1f48dafc111b0a488fa454aca95e0b5e::usdc::USDC".to_string()),
                )],
                json!(result),
            )],
        )
    }

    fn sui_get_latest_checkpoint_sequence_number(&mut self) -> Examples {
        let result = "507021";
        Examples::new(
            "sui_getLatestCheckpointSequenceNumber",
            vec![ExamplePairing::new(
                "Gets the sequence number for the latest checkpoint.",
                vec![],
                json!(result),
            )],
        )
    }

    fn suix_get_coins(&mut self) -> Examples {
        let coin_type = "0x2::sui::SUI".to_string();
        let owner = SuiAddress::from(ObjectID::new(self.rng.gen()));
        let coins = (0..3)
            .map(|_| Coin {
                coin_type: coin_type.clone(),
                coin_object_id: ObjectID::new(self.rng.gen()),
                version: SequenceNumber::from_u64(103626),
                digest: ObjectDigest::new(self.rng.gen()),
                balance: 200000000,
                //locked_until_epoch: None,
                previous_transaction: TransactionDigest::new(self.rng.gen()),
            })
            .collect::<Vec<_>>();

        let page = CoinPage {
            data: coins,
            next_cursor: Some("abcd".to_string()),
            has_next_page: true,
        };

        Examples::new(
            "suix_getCoins",
            vec![ExamplePairing::new(
                "Gets all SUI coins owned by the address provided. Return a paginated list of `limit` results per page. Similar to `suix_getAllCoins`, but provides a way to filter by coin type.",
                vec![
                    ("owner", json!(owner)),
                    ("coin_type", json!(coin_type)),
                    ("cursor", json!(ObjectID::new(self.rng.gen()))),
                    ("limit", json!(3)),
                ],
                json!(page),
            )],
        )
    }

    fn suix_get_total_supply(&mut self) -> Examples {
        let mut coin = ObjectID::new(self.rng.gen()).to_string();
        coin.push_str("::acoin::ACOIN");

        let result = Supply { value: 12023692 };

        Examples::new(
            "suix_getTotalSupply",
            vec![ExamplePairing::new(
                "Gets total supply for the type of coin provided.",
                vec![("coin_type", json!(coin))],
                json!(result),
            )],
        )
    }

    fn sui_get_move_function_arg_types(&mut self) -> Examples {
        let result = vec![
            MoveFunctionArgType::Object(ByMutableReference),
            MoveFunctionArgType::Pure,
            MoveFunctionArgType::Pure,
            MoveFunctionArgType::Object(ByValue),
            MoveFunctionArgType::Object(ByImmutableReference),
            MoveFunctionArgType::Object(ByValue),
            MoveFunctionArgType::Object(ByMutableReference),
        ];

        Examples::new(
            "sui_getMoveFunctionArgTypes",
            vec![ExamplePairing::new(
                "Returns the argument types for the package and function the request provides.",
                vec![
                    ("package", json!(ObjectID::new(self.rng.gen()))),
                    ("module", json!("suifrens".to_string())),
                    ("function", json!("mint".to_string())),
                ],
                json!(result),
            )],
        )
    }

    fn sui_get_normalized_move_function(&mut self) -> Examples {
        let ability_set = SuiMoveAbilitySet {
            abilities: vec![SuiMoveAbility::Store, SuiMoveAbility::Key],
        };

        let result = SuiMoveNormalizedFunction {
            is_entry: false,
            type_parameters: vec![ability_set],
            parameters: vec![SuiMoveNormalizedType::U64],
            visibility: SuiMoveVisibility::Public,
            return_: vec![SuiMoveNormalizedType::U64],
        };

        Examples::new(
            "sui_getNormalizedMoveFunction",
            vec![ExamplePairing::new(
                "Returns the structured representation of the function the request provides.",
                vec![
                    ("package", json!(ObjectID::new(self.rng.gen()))),
                    ("module_name", json!("moduleName".to_string())),
                    ("function_name", json!("functionName".to_string())),
                ],
                json!(result),
            )],
        )
    }

    fn sui_get_normalized_move_module(&mut self) -> Examples {
        let result = SuiMoveNormalizedModule {
            address: ObjectID::new(self.rng.gen()).to_string(),
            exposed_functions: BTreeMap::new(),
            file_format_version: 6,
            friends: vec![],
            name: "module".to_string(),
            structs: BTreeMap::new(),
            enums: BTreeMap::new(),
        };

        Examples::new(
            "sui_getNormalizedMoveModule",
            vec![ExamplePairing::new(
                "Gets a structured representation of the Move module for the package in the request.",
                vec![
                    ("package", json!(ObjectID::new(self.rng.gen()))),
                    ("module_name", json!("module".to_string())),
                ],
                json!(result),
            )],
        )
    }

    fn sui_get_normalized_move_modules_by_package(&mut self) -> Examples {
        let result = SuiMoveNormalizedModule {
            address: ObjectID::new(self.rng.gen()).to_string(),
            exposed_functions: BTreeMap::new(),
            file_format_version: 6,
            friends: vec![],
            name: "module".to_string(),
            structs: BTreeMap::new(),
            enums: BTreeMap::new(),
        };

        Examples::new(
            "sui_getNormalizedMoveModulesByPackage",
            vec![ExamplePairing::new(
                "Gets structured representations of all the modules for the package in the request.",
                vec![
                    ("package", json!(ObjectID::new(self.rng.gen()))),
                ],
                json!(result),
            )],
        )
    }

    fn sui_get_normalized_move_struct(&mut self) -> Examples {
        let abilities = SuiMoveAbilitySet {
            abilities: vec![SuiMoveAbility::Store, SuiMoveAbility::Key],
        };
        let fields = vec![].into_iter().collect::<Vec<_>>();
        let type_parameters = vec![].into_iter().collect::<Vec<_>>();
        let result = SuiMoveNormalizedStruct {
            abilities,
            fields,
            type_parameters,
        };

        Examples::new(
            "sui_getNormalizedMoveStruct",
            vec![ExamplePairing::new(
                "Gets a structured representation of the struct in the request.",
                vec![
                    ("package", json!(ObjectID::new(self.rng.gen()))),
                    ("module_name", json!("module".to_string())),
                    ("struct_name", json!("StructName".to_string())),
                ],
                json!(result),
            )],
        )
    }

    fn suix_get_validators_apy(&mut self) -> Examples {
        let result = vec![
            ValidatorApy {
                address: SuiAddress::from(ObjectID::new(self.rng.gen())),
                apy: 0.06,
            },
            ValidatorApy {
                address: SuiAddress::from(ObjectID::new(self.rng.gen())),
                apy: 0.02,
            },
            ValidatorApy {
                address: SuiAddress::from(ObjectID::new(self.rng.gen())),
                apy: 0.05,
            },
        ];

        Examples::new(
            "suix_getValidatorsApy",
            vec![ExamplePairing::new(
                "Gets the APY for all validators.",
                vec![],
                json!(ValidatorApys {
                    apys: result,
                    epoch: 420
                }),
            )],
        )
    }

    fn suix_get_dynamic_fields(&mut self) -> Examples {
        let object_id = ObjectID::new(self.rng.gen());
        let dynamic_fields = (0..3)
            .map(|_| DynamicFieldInfo {
                name: DynamicFieldName {
                    type_: TypeTag::from_str("0x9::test::TestField").unwrap(),
                    value: serde_json::Value::String("some_value".to_string()),
                },
                bcs_name: bcs::to_bytes("0x9::test::TestField").unwrap(),
                type_: DynamicFieldType::DynamicField,
                object_type: "test".to_string(),
                object_id: ObjectID::new(self.rng.gen()),
                version: SequenceNumber::from_u64(1),
                digest: ObjectDigest::new(self.rng.gen()),
            })
            .map(Into::into)
            .collect::<Vec<_>>();

        let next_cursor = ObjectID::new(self.rng.gen());

        let page = DynamicFieldPage {
            data: dynamic_fields,
            next_cursor: Some(next_cursor),
            has_next_page: true,
        };

        Examples::new("suix_getDynamicFields",
        vec![ExamplePairing::new(
            "Gets dynamic fields for the object the request provides in a paginated list of `limit` dynamic field results per page. The default limit is 50.",
            vec![
                ("parent_object_id", json!(object_id)),
                ("cursor", json!(ObjectID::new(self.rng.gen()))),
                ("limit", json!(3)),
            ],
            json!(page),
        )],)
    }

    fn suix_get_dynamic_field_object(&mut self) -> Examples {
        let parent_object_id = ObjectID::new(self.rng.gen());
        let field_name = DynamicFieldName {
            type_: TypeTag::from_str("0x9::test::TestField").unwrap(),
            value: serde_json::Value::String("some_value".to_string()),
        };

        let struct_tag = parse_sui_struct_tag("0x9::test::TestField").unwrap();
        let resp = SuiObjectResponse::new_with_data(SuiObjectData {
            content: Some(
                SuiParsedData::try_from_object(
                    unsafe {
                        MoveObject::new_from_execution_with_limit(
                            MoveObjectType::from(struct_tag.clone()),
                            true,
                            SequenceNumber::from_u64(1),
                            Vec::new(),
                            5,
                        )
                        .unwrap()
                    },
                    MoveStructLayout {
                        type_: struct_tag,
                        fields: Vec::new(),
                    },
                )
                .unwrap(),
            ),
            owner: Some(Owner::AddressOwner(SuiAddress::from(ObjectID::new(
                self.rng.gen(),
            )))),
            previous_transaction: Some(TransactionDigest::new(self.rng.gen())),
            storage_rebate: Some(100),
            object_id: parent_object_id,
            version: SequenceNumber::from_u64(1),
            digest: ObjectDigest::new(self.rng.gen()),
            type_: Some(ObjectType::Struct(MoveObjectType::from(
                parse_sui_struct_tag("0x9::test::TestField").unwrap(),
            ))),
            bcs: None,
            display: None,
        });
        Examples::new(
            "suix_getDynamicFieldObject",
            vec![ExamplePairing::new(
                "Gets the information for the dynamic field the request provides.",
                vec![
                    ("parent_object_id", json!(parent_object_id)),
                    ("name", json!(field_name)),
                ],
                json!(resp),
            )],
        )
    }

    fn suix_get_owned_objects(&mut self) -> Examples {
        let owner = SuiAddress::from(ObjectID::new(self.rng.gen()));
        let version: u64 = 13488;
        let options = Some(
            SuiObjectDataOptions::new()
                .with_type()
                .with_owner()
                .with_previous_transaction(),
        );
        let filter = Some(SuiObjectDataFilter::MatchAll(vec![
            SuiObjectDataFilter::StructType(
                StructTag::from_str("0x2::coin::Coin<0x2::sui::SUI>").unwrap(),
            ),
            SuiObjectDataFilter::AddressOwner(owner),
            SuiObjectDataFilter::Version(version),
        ]));
        let query = json!(SuiObjectResponseQuery { filter, options });
        let object_id = ObjectID::new(self.rng.gen());

        let items = (0..3)
            .map(|_| {
                SuiObjectResponse::new_with_data(SuiObjectData {
                    content: None,
                    owner: Some(Owner::AddressOwner(owner)),
                    previous_transaction: Some(TransactionDigest::new(self.rng.gen())),
                    storage_rebate: Some(100),
                    object_id: ObjectID::new(self.rng.gen()),
                    version: SequenceNumber::from_u64(version),
                    digest: ObjectDigest::new(self.rng.gen()),
                    type_: Some(ObjectType::Struct(MoveObjectType::gas_coin())),
                    bcs: None,
                    display: None,
                })
            })
            .collect::<Vec<_>>();

        let next_cursor = items.last().unwrap().object_id();
        let result = ObjectsPage {
            data: items,
            next_cursor: Some(next_cursor.unwrap()),
            has_next_page: true,
        };

        Examples::new(
            "suix_getOwnedObjects",
            vec![ExamplePairing::new(
                "Returns all the objects the address provided in the request owns and that match the filter. By default, only the digest value is returned, but the request returns additional information by setting the relevant keys to true. A cursor value is also provided, so the list of results begin after that value.",
                vec![
                    ("address", json!(owner)),
                    ("query", json!(query)),
                    ("cursor", json!(object_id)),
                    ("limit", json!(3))
                ],
                json!(result),
            )],
        )
    }

    fn suix_query_events(&mut self) -> Examples {
        let package_id = ObjectID::new(self.rng.gen());
        let identifier = Identifier::from_str("test").unwrap();
        let mut event_ids = self.get_event_ids(5..9);
        let has_next_page = event_ids.len() > (9 - 5);
        event_ids.truncate(9 - 5);
        let next_cursor = event_ids.last().cloned();
        let cursor = event_ids.last().cloned();

        let data = event_ids
            .into_iter()
            .map(|event_id| SuiEvent {
                id: event_id,
                package_id,
                transaction_module: identifier.clone(),
                sender: SuiAddress::from(ObjectID::new(self.rng.gen())),
                type_: StructTag::from_str("0x3::test::Test<0x3::test::Test>").unwrap(),
                parsed_json: serde_json::Value::String("some_value".to_string()),
                bcs: BcsEvent::new(vec![]),
                timestamp_ms: None,
            })
            .collect();

        let result = EventPage {
            data,
            next_cursor,
            has_next_page,
        };
        Examples::new(
            "suix_queryEvents",
            vec![ExamplePairing::new(
                "Returns the events for a specified query criteria.",
                vec![
                    (
                        "query",
                        json!(EventFilter::MoveModule {
                            package: ObjectID::new(self.rng.gen()),
                            module: Identifier::from_str("test").unwrap(),
                        }),
                    ),
                    ("cursor", json!(cursor)),
                    ("limit", json!(100)),
                    ("descending_order", json!(false)),
                ],
                json!(result),
            )],
        )
    }

    fn suix_get_latest_sui_system_state(&mut self) -> Examples {
        let result = "some_system_state";
        Examples::new(
            "suix_getLatestSuiSystemState",
            vec![ExamplePairing::new(
                "Gets objects owned by the address in the request.",
                vec![],
                json!(result),
            )],
        )
    }

    fn sui_get_chain_identifier(&mut self) -> Examples {
        let result = "4c78adac".to_string();
        Examples::new(
            "sui_getChainIdentifier",
            vec![ExamplePairing::new(
                "Gets the identifier for the chain receiving the POST.",
                vec![],
                json!(result),
            )],
        )
    }

    fn suix_get_stakes(&mut self) -> Examples {
        let principal = 200000000000;
        let owner = SuiAddress::from(ObjectID::new(self.rng.gen()));
        let result = vec![
            DelegatedStake {
                validator_address: SuiAddress::from(ObjectID::new(self.rng.gen())),
                staking_pool: ObjectID::new(self.rng.gen()),
                stakes: vec![
                    Stake {
                        staked_sui_id: ObjectID::new(self.rng.gen()),
                        stake_request_epoch: 62,
                        stake_active_epoch: 63,
                        principal,
                        status: StakeStatus::Active {
                            estimated_reward: (principal as f64 * 0.0026) as u64,
                        },
                    },
                    Stake {
                        staked_sui_id: ObjectID::new(self.rng.gen()),
                        stake_request_epoch: 142,
                        stake_active_epoch: 143,
                        principal,
                        status: StakeStatus::Pending,
                    },
                ],
            },
            DelegatedStake {
                validator_address: SuiAddress::from(ObjectID::new(self.rng.gen())),
                staking_pool: ObjectID::new(self.rng.gen()),
                stakes: vec![Stake {
                    staked_sui_id: ObjectID::new(self.rng.gen()),
                    stake_request_epoch: 244,
                    stake_active_epoch: 245,
                    principal,
                    status: StakeStatus::Unstaked,
                }],
            },
        ];

        Examples::new(
            "suix_getStakes",
            vec![ExamplePairing::new(
                "Returns the staking information for the address the request provides.",
                vec![("owner", json!(owner))],
                json!(result),
            )],
        )
    }

    fn suix_get_stakes_by_ids(&mut self) -> Examples {
        let principal = 200000000000;
        let stake1 = ObjectID::new(self.rng.gen());
        let stake2 = ObjectID::new(self.rng.gen());
        let result = DelegatedStake {
            validator_address: SuiAddress::from(ObjectID::new(self.rng.gen())),
            staking_pool: ObjectID::new(self.rng.gen()),
            stakes: vec![
                Stake {
                    staked_sui_id: stake1,
                    stake_request_epoch: 62,
                    stake_active_epoch: 63,
                    principal,
                    status: StakeStatus::Active {
                        estimated_reward: (principal as f64 * 0.0026) as u64,
                    },
                },
                Stake {
                    staked_sui_id: stake2,
                    stake_request_epoch: 244,
                    stake_active_epoch: 245,
                    principal,
                    status: StakeStatus::Unstaked,
                },
            ],
        };
        Examples::new(
            "suix_getStakesByIds",
            vec![ExamplePairing::new(
                "Returns the staking information for the address the request provides.",
                vec![("staked_sui_ids", json!(vec![stake1, stake2]))],
                json!(result),
            )],
        )
    }

    fn suix_resolve_name_service_address(&mut self) -> Examples {
        let result = ObjectID::new(self.rng.gen());
        Examples::new(
            "suix_resolveNameServiceAddress",
            vec![ExamplePairing::new(
                "Returns the resolved address for the name the request provides.",
                vec![("name", json!("example.sui".to_string()))],
                json!(result),
            )],
        )
    }

    fn suix_resolve_name_service_names(&mut self) -> Examples {
        let next_cursor = Some(ObjectID::new(self.rng.gen()));
        let object_id = ObjectID::new(self.rng.gen());
        let result = Page {
            data: vec!["example.sui".to_string()],
            next_cursor,
            has_next_page: false,
        };
        Examples::new(
            "suix_resolveNameServiceNames",
            vec![ExamplePairing::new(
                "Returns the SuiNS name for the address the request provides. Currently, the API returns only the first name in cases where there are multiple. Future support will use the cursor ID and limit values in the request to control pagination of the response for addresses with multiple names.",
                vec![
                    ("address", json!(object_id)),
                    ("cursor", json!(next_cursor)),
                    ("limit", json!(3)),
                ],
                json!(result),
            )],
        )
    }

    fn sui_try_multi_get_past_objects(&mut self) -> Examples {
        let object_id = ObjectID::new(self.rng.gen());
        let object_id2 = ObjectID::new(self.rng.gen());
        let version = SequenceNumber::from_u64(4);
        let version2 = SequenceNumber::from_u64(12);
        let objects = vec![
            SuiGetPastObjectRequest { object_id, version },
            SuiGetPastObjectRequest {
                object_id: object_id2,
                version: version2,
            },
        ];
        let coin = GasCoin::new(object_id, 10000);
        let coin2 = GasCoin::new(object_id, 20000);
        let result = vec![
            SuiPastObjectResponse::VersionFound(SuiObjectData {
                content: Some(
                    SuiParsedData::try_from_object(
                        coin.to_object(SequenceNumber::from_u64(1)),
                        GasCoin::layout(),
                    )
                    .unwrap(),
                ),
                owner: Some(Owner::AddressOwner(SuiAddress::from(ObjectID::new(
                    self.rng.gen(),
                )))),
                previous_transaction: Some(TransactionDigest::new(self.rng.gen())),
                storage_rebate: Some(100),
                object_id,
                version: SequenceNumber::from_u64(4),
                digest: ObjectDigest::new(self.rng.gen()),
                type_: Some(ObjectType::Struct(MoveObjectType::gas_coin())),
                bcs: None,
                display: None,
            }),
            SuiPastObjectResponse::VersionFound(SuiObjectData {
                content: Some(
                    SuiParsedData::try_from_object(
                        coin2.to_object(SequenceNumber::from_u64(4)),
                        GasCoin::layout(),
                    )
                    .unwrap(),
                ),
                owner: Some(Owner::AddressOwner(SuiAddress::from(ObjectID::new(
                    self.rng.gen(),
                )))),
                previous_transaction: Some(TransactionDigest::new(self.rng.gen())),
                storage_rebate: Some(100),
                object_id: object_id2,
                version: version2,
                digest: ObjectDigest::new(self.rng.gen()),
                type_: Some(ObjectType::Struct(MoveObjectType::gas_coin())),
                bcs: None,
                display: None,
            }),
        ];

        Examples::new(
            "sui_tryMultiGetPastObjects",
            vec![ExamplePairing::new(
                "Gets Past Object data for a vector of objects.",
                vec![
                    ("past_objects", json!(objects)),
                    ("options", json!(SuiObjectDataOptions::full_content())),
                ],
                json!(result),
            )],
        )
    }
}
