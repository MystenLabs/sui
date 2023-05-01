// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::collections::HashMap;
use std::ops::Range;
use std::str::FromStr;

use fastcrypto::traits::EncodeDecodeBase64;
use move_core_types::identifier::Identifier;
use move_core_types::language_storage::ModuleId;
use move_core_types::language_storage::StructTag;
use move_core_types::resolver::ModuleResolver;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use serde_json::json;

use sui_json::SuiJsonValue;
use sui_json_rpc::error::Error;
use sui_json_rpc_types::SuiTypeTag;
use sui_json_rpc_types::TransactionFilter;
use sui_json_rpc_types::{
    Balance, Checkpoint, CheckpointId, CheckpointPage, Coin, CoinPage, EventPage, MoveCallParams,
    ObjectChange, OwnedObjectRef, RPCTransactionRequestParams, SuiCommittee, SuiData, SuiEvent,
    SuiExecutionStatus, SuiObjectData, SuiObjectDataFilter, SuiObjectDataOptions, SuiObjectRef,
    SuiObjectResponse, SuiObjectResponseQuery, SuiParsedData, SuiPastObjectResponse,
    SuiTransactionBlock, SuiTransactionBlockData, SuiTransactionBlockEffects,
    SuiTransactionBlockEffectsV1, SuiTransactionBlockResponse, SuiTransactionBlockResponseOptions,
    SuiTransactionBlockResponseQuery, TransactionBlockBytes, TransactionBlocksPage,
    TransferObjectParams,
};
use sui_open_rpc::ExamplePairing;
use sui_types::balance::Supply;
use sui_types::base_types::random_object_ref;
use sui_types::base_types::{
    MoveObjectType, ObjectDigest, ObjectID, ObjectType, SequenceNumber, SuiAddress,
    TransactionDigest,
};
use sui_types::coin::CoinMetadata;
use sui_types::committee::Committee;
use sui_types::crypto::{get_key_pair_from_rng, AccountKeyPair, AggregateAuthoritySignature};
use sui_types::digests::TransactionEventsDigest;
use sui_types::event::EventID;
use sui_types::gas::GasCostSummary;
use sui_types::gas_coin::GasCoin;
use sui_types::id::UID;
use sui_types::messages::ObjectArg;
use sui_types::messages::TEST_ONLY_GAS_UNIT_FOR_TRANSFER;
use sui_types::messages::{CallArg, ExecuteTransactionRequestType, TransactionData};
use sui_types::messages_checkpoint::CheckpointDigest;
use sui_types::object::Owner;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::signature::GenericSignature;
use sui_types::utils::to_sender_signed_transaction;
use sui_types::{parse_sui_struct_tag, SUI_FRAMEWORK_OBJECT_ID};
struct Examples {
    function_name: String,
    examples: Vec<ExamplePairing>,
}

#[derive(serde::Serialize)]
struct Value {
    value: String,
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
                package_object_id: SUI_FRAMEWORK_OBJECT_ID,
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
                    SUI_FRAMEWORK_OBJECT_ID,
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
                "Create unsigned batch transaction data.",
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
                "Execute a transaction with serialized signatures.",
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

    fn get_object_example(&mut self) -> Examples {
        let object_id = ObjectID::new(self.rng.gen());

        let coin = GasCoin::new(object_id, 10000);

        let result = SuiObjectResponse::new_with_data(SuiObjectData {
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
        });

        Examples::new(
            "sui_getObject",
            vec![ExamplePairing::new(
                "Get Object data for the ID in the request.",
                vec![
                    ("object_id", json!(object_id)),
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
                "Get Past Object data.",
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
                "Get checkpoint information for the checkpoint ID in the request.",
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
                "Get a paginated list in descending order of all checkpoints starting at the provided cursor. Each page of results has a maximum number of checkpoints set by the provided limit.",
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
                "Get objects owned by the address in the request.",
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
                "Get total number of transactions on the network.",
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
                "Return the transaction response object for specified transaction digest.",
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
                "Return the transaction digest for specified query criteria.",
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

    fn get_transaction_digests(&mut self, range: Range<u64>) -> Vec<TransactionDigest> {
        range
            .into_iter()
            .map(|_| TransactionDigest::new(self.rng.gen()))
            .collect()
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
        let tx1 = tx.clone();
        let signatures = tx.into_inner().tx_signatures().to_vec();
        let raw_transaction = bcs::to_bytes(tx1.data()).unwrap();

        let tx_digest = tx1.digest();
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
        };

        (data2, signatures, recipient, obj_id, result)
    }

    fn get_events(&mut self) -> Examples {
        let (_, _, _, _, result) = self.get_transfer_data_response();
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
            parsed_json: json! ({"test": "example value"}),
            bcs: vec![],
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
                "Return the events the transaction in the request emits.",
                vec![("transaction_digest", json!(result.digest))],
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
                "Get committee information for epoch 5000.",
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
                "Get reference gas price information for the network.",
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
                "Get all balances for the address in the request.",
                vec![("owner", json!(address))],
                json!(vec![result]),
            )],
        )
    }

    fn suix_get_all_coins(&mut self) -> Examples {
        let limit = 3;
        let owner = SuiAddress::from(ObjectID::new(self.rng.gen()));
        let cursor = ObjectID::new(self.rng.gen());
        let next = ObjectID::new(self.rng.gen());
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
            next_cursor: Some(next),
            has_next_page: true,
        };

        Examples::new(
            "suix_getAllCoins",
            vec![ExamplePairing::new(
                "Get all coins for the address in the request body. Begin listing the coins that are after the provided `cursor` value and return only the `limit` amount of results per page.",
                vec![
                        ("owner", json!(owner)), 
                        ("cursor", json!(cursor)), 
                        ("limit", json!(limit))
                    ],
                json!(page),
            )]
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
                "Get the balance of the specified type of coin for the address in the request.",
                vec![("owner", json!(owner)), ("coin_type", json!(coin_type))],
                json!(result),
            )],
        )
    }

    fn suix_get_coin_metadata(&mut self) -> Examples {
        let id = UID::new(ObjectID::new(self.rng.gen()));

        let result = CoinMetadata {
            decimals: 9,
            name: "Usdc".to_string(),
            symbol: "USDC".to_string(),
            description: "Stable coin.".to_string(),
            icon_url: None,
            id,
        };

        Examples::new(
            "suix_getCoinMetadata",
            vec![ExamplePairing::new(
                "Get the metadata for the coin type in the request.",
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
                "Get the sequence number for the latest checkpoint.",
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

        let next_cursor = coins.last().unwrap().coin_object_id;

        let page = CoinPage {
            data: coins,
            next_cursor: Some(next_cursor),
            has_next_page: true,
        };

        Examples::new(
            "suix_getCoins",
            vec![ExamplePairing::new(
                "Get all SUI coins owned by the address provided. Return a paginated list of `limit` results per page. Similar to `suix_getAllCoins`, but provides a way to filter by coin type.",
                vec![
                    ("owner", json!(owner)),
                    ("coin_type", json!(coin_type)),
                    ("cursor", json!(ObjectID::new(self.rng.gen()))),
                    ("limit", json!(3))
                ],
                json!(page)
            )]
        )
    }

    fn suix_get_total_supply(&mut self) -> Examples {
        let mut coin = ObjectID::new(self.rng.gen()).to_string();
        coin.push_str("::acoin::ACOIN");

        let result = Supply { value: 12023692 };

        Examples::new(
            "suix_getTotalSupply",
            vec![ExamplePairing::new(
                "Get total supply for the type of coin provided.",
                vec![("coin_type", json!(coin))],
                json!(result),
            )],
        )
    }
}
