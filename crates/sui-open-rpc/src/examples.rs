// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::identifier::Identifier;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use serde_json::json;
use std::collections::BTreeMap;
use std::ops::Range;
use std::str::FromStr;
use sui_types::intent::{Intent, IntentMessage};

use sui::client_commands::EXAMPLE_NFT_DESCRIPTION;
use sui::client_commands::EXAMPLE_NFT_NAME;
use sui::client_commands::EXAMPLE_NFT_URL;
use sui_core::test_utils::to_sender_signed_transaction;
use sui_json::SuiJsonValue;
use sui_json_rpc_types::{
    MoveCallParams, OwnedObjectRef, RPCTransactionRequestParams, SuiCertifiedTransaction, SuiData,
    SuiEvent, SuiEventEnvelope, SuiExecutionStatus, SuiGasCostSummary, SuiObject, SuiObjectRead,
    SuiObjectRef, SuiParsedData, SuiPastObjectRead, SuiRawData, SuiRawMoveObject,
    SuiTransactionData, SuiTransactionEffects, SuiTransactionResponse, TransactionBytes,
    TransactionsPage, TransferObjectParams,
};
use sui_open_rpc::ExamplePairing;
use sui_types::base_types::{
    ObjectDigest, ObjectID, ObjectInfo, SequenceNumber, SuiAddress, TransactionDigest,
};
use sui_types::crypto::{get_key_pair_from_rng, AccountKeyPair, Signature};
use sui_types::crypto::{AuthorityQuorumSignInfo, SuiSignature};
use sui_types::event::TransferType;
use sui_types::gas_coin::GasCoin;
use sui_types::messages::{
    CallArg, ExecuteTransactionRequestType, MoveCall, SingleTransactionKind, Transaction,
    TransactionData, TransactionKind, TransferObject,
};
use sui_types::object::Owner;
use sui_types::query::Ordering;
use sui_types::query::TransactionQuery;
use sui_types::sui_serde::{Base64, Encoding};
use sui_types::SUI_FRAMEWORK_OBJECT_ID;

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
            self.execute_transaction_example(),
            self.get_object_example(),
            self.get_past_object_example(),
            self.get_objects_owned_by_address(),
            self.get_objects_owned_by_object(),
            self.get_raw_object(),
            self.get_recent_transactions(),
            self.get_total_transaction_number(),
            self.get_transaction(),
            self.get_transactions(),
            self.get_events_by_transaction(),
            self.get_events_by_object(),
            self.get_events_by_sender(),
            self.get_events_by_recipient(),
            self.get_events_by_move_event_struct_name(),
            self.get_events_by_transaction_module(),
            self.get_events_by_timerange(),
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

        let tx_params = vec![
            RPCTransactionRequestParams::MoveCallRequestParams(MoveCallParams {
                package_object_id: SUI_FRAMEWORK_OBJECT_ID,
                module: "devnet_nft".to_string(),
                function: "mint".to_string(),
                type_arguments: vec![],
                arguments: vec![
                    SuiJsonValue::new(json!(EXAMPLE_NFT_NAME)).unwrap(),
                    SuiJsonValue::new(json!(EXAMPLE_NFT_DESCRIPTION)).unwrap(),
                    SuiJsonValue::new(json!(EXAMPLE_NFT_URL)).unwrap(),
                ],
            }),
            RPCTransactionRequestParams::TransferObjectRequestParams(TransferObjectParams {
                recipient,
                object_id,
            }),
        ];

        let data = TransactionData::new(
            TransactionKind::Batch(vec![
                SingleTransactionKind::Call(MoveCall {
                    package: (
                        SUI_FRAMEWORK_OBJECT_ID,
                        SequenceNumber::from_u64(1),
                        ObjectDigest::new(self.rng.gen()),
                    ),
                    module: Identifier::from_str("devnet_nft").unwrap(),
                    function: Identifier::from_str("mint").unwrap(),
                    type_arguments: vec![],
                    arguments: vec![
                        CallArg::Pure(EXAMPLE_NFT_NAME.as_bytes().to_vec()),
                        CallArg::Pure(EXAMPLE_NFT_DESCRIPTION.as_bytes().to_vec()),
                        CallArg::Pure(EXAMPLE_NFT_URL.as_bytes().to_vec()),
                    ],
                }),
                SingleTransactionKind::TransferObject(TransferObject {
                    recipient,
                    object_ref: (
                        object_id,
                        SequenceNumber::from_u64(1),
                        ObjectDigest::new(self.rng.gen()),
                    ),
                }),
            ]),
            signer,
            (
                gas_id,
                SequenceNumber::from_u64(1),
                ObjectDigest::new(self.rng.gen()),
            ),
            1000,
        );

        let result = TransactionBytes::from_data(data).unwrap();

        Examples::new(
            "sui_batchTransaction",
            vec![ExamplePairing::new(
                "Create unsigned batch transaction data.",
                vec![
                    ("signer", json!(signer)),
                    ("single_transaction_params", json!(tx_params)),
                    ("gas", json!(gas_id)),
                    ("gas_budget", json!(1000)),
                ],
                json!(result),
            )],
        )
    }

    fn execute_transaction_example(&mut self) -> Examples {
        let (data, intent, signature, _, _, result, _) = self.get_transfer_data_response();
        let intent_msg = IntentMessage::new(intent, data);

        Examples::new(
            "sui_executeTransaction",
            vec![ExamplePairing::new(
                "Execute an object transfer transaction",
                vec![
                    (
                        "tx_bytes",
                        json!(Base64::encode(bcs::to_bytes(&intent_msg).unwrap())),
                    ),
                    ("sig_scheme", json!(signature.scheme())),
                    (
                        "signature",
                        json!(Base64::from_bytes(signature.signature_bytes())),
                    ),
                    (
                        "pub_key",
                        json!(Base64::from_bytes(signature.public_key_bytes())),
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

        let result = SuiObjectRead::Exists(SuiObject {
            data: SuiParsedData::try_from_object(
                coin.to_object(SequenceNumber::from_u64(1)),
                GasCoin::layout(),
            )
            .unwrap(),
            owner: Owner::AddressOwner(SuiAddress::from(ObjectID::new(self.rng.gen()))),
            previous_transaction: TransactionDigest::new(self.rng.gen()),
            storage_rebate: 100,
            reference: SuiObjectRef::from((
                object_id,
                SequenceNumber::from_u64(1),
                ObjectDigest::new(self.rng.gen()),
            )),
        });

        Examples::new(
            "sui_getObject",
            vec![ExamplePairing::new(
                "Get Object data",
                vec![("object_id", json!(object_id))],
                json!(result),
            )],
        )
    }

    fn get_past_object_example(&mut self) -> Examples {
        let object_id = ObjectID::new(self.rng.gen());

        let coin = GasCoin::new(object_id, 10000);

        let result = SuiPastObjectRead::VersionFound(SuiObject {
            data: SuiParsedData::try_from_object(
                coin.to_object(SequenceNumber::from_u64(1)),
                GasCoin::layout(),
            )
            .unwrap(),
            owner: Owner::AddressOwner(SuiAddress::from(ObjectID::new(self.rng.gen()))),
            previous_transaction: TransactionDigest::new(self.rng.gen()),
            storage_rebate: 100,
            reference: SuiObjectRef::from((
                object_id,
                SequenceNumber::from_u64(4),
                ObjectDigest::new(self.rng.gen()),
            )),
        });

        Examples::new(
            "sui_tryGetPastObject",
            vec![ExamplePairing::new(
                "Get Past Object data",
                vec![("object_id", json!(object_id)), ("version", json!(4))],
                json!(result),
            )],
        )
    }

    fn get_objects_owned_by_address(&mut self) -> Examples {
        let owner = SuiAddress::from(ObjectID::new(self.rng.gen()));
        let result = (0..4)
            .map(|_| ObjectInfo {
                object_id: ObjectID::new(self.rng.gen()),
                version: Default::default(),
                digest: ObjectDigest::new(self.rng.gen()),
                type_: GasCoin::type_().to_string(),
                owner: Owner::AddressOwner(owner),
                previous_transaction: TransactionDigest::new(self.rng.gen()),
            })
            .collect::<Vec<_>>();

        Examples::new(
            "sui_getObjectsOwnedByAddress",
            vec![ExamplePairing::new(
                "Get objects owned by an address",
                vec![("address", json!(owner))],
                json!(result),
            )],
        )
    }
    fn get_objects_owned_by_object(&mut self) -> Examples {
        let owner = ObjectID::new(self.rng.gen());
        let result = (0..4)
            .map(|_| ObjectInfo {
                object_id: ObjectID::new(self.rng.gen()),
                version: Default::default(),
                digest: ObjectDigest::new(self.rng.gen()),
                type_: GasCoin::type_().to_string(),
                owner: Owner::ObjectOwner(SuiAddress::from(owner)),
                previous_transaction: TransactionDigest::new(self.rng.gen()),
            })
            .collect::<Vec<_>>();

        Examples::new(
            "sui_getObjectsOwnedByObject",
            vec![ExamplePairing::new(
                "Get objects owned by an object",
                vec![("object_id", json!(owner))],
                json!(result),
            )],
        )
    }

    fn get_raw_object(&mut self) -> Examples {
        let object_id = ObjectID::new(self.rng.gen());

        let coin = GasCoin::new(object_id, 10000);
        let object = coin.to_object(SequenceNumber::from_u64(1));
        let result = SuiObjectRead::Exists(SuiObject {
            data: SuiRawData::MoveObject(SuiRawMoveObject {
                type_: GasCoin::type_().to_string(),
                has_public_transfer: object.has_public_transfer(),
                version: object.version(),
                bcs_bytes: object.into_contents(),
            }),
            owner: Owner::AddressOwner(SuiAddress::from(ObjectID::new(self.rng.gen()))),
            previous_transaction: TransactionDigest::new(self.rng.gen()),
            storage_rebate: 100,
            reference: SuiObjectRef::from((
                object_id,
                SequenceNumber::from_u64(1),
                ObjectDigest::new(self.rng.gen()),
            )),
        });

        Examples::new(
            "sui_getRawObject",
            vec![ExamplePairing::new(
                "Get Raw Object data",
                vec![("object_id", json!(object_id))],
                json!(result),
            )],
        )
    }

    fn get_recent_transactions(&mut self) -> Examples {
        let result = self.get_transaction_digests(5..10);
        Examples::new(
            "sui_getRecentTransactions",
            vec![ExamplePairing::new(
                "Get recent transactions",
                vec![("count", json!(5))],
                json!(result),
            )],
        )
    }

    fn get_total_transaction_number(&mut self) -> Examples {
        Examples::new(
            "sui_getTotalTransactionNumber",
            vec![ExamplePairing::new(
                "Get total number of transactions",
                vec![],
                json!(100),
            )],
        )
    }

    fn get_transaction(&mut self) -> Examples {
        let (_, _, _, _, _, result, _) = self.get_transfer_data_response();
        Examples::new(
            "sui_getTransaction",
            vec![ExamplePairing::new(
                "Return the transaction response object for specified transaction digest",
                vec![(
                    "digest",
                    json!(result.certificate.transaction_digest.clone()),
                )],
                json!(result),
            )],
        )
    }

    fn get_transactions(&mut self) -> Examples {
        let mut data = self.get_transaction_digests(5..9);
        let next_cursor = data.pop();

        let result = TransactionsPage { data, next_cursor };
        Examples::new(
            "sui_getTransactions",
            vec![ExamplePairing::new(
                "Return the transaction digest for specified query criteria",
                vec![
                    (
                        "query",
                        json!(TransactionQuery::InputObject(ObjectID::new(self.rng.gen()))),
                    ),
                    ("cursor", json!(10)),
                    ("limit", json!(100)),
                    ("order", json!(Ordering::Ascending)),
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
        Intent,
        Signature,
        SuiAddress,
        ObjectID,
        SuiTransactionResponse,
        Vec<SuiEventEnvelope>,
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

        let data = TransactionData::new_transfer(recipient, object_ref, signer, gas_ref, 1000);
        let data1 = data.clone();
        let data2 = data.clone();

        let tx = to_sender_signed_transaction(data, &kp);
        let tx1 = tx.clone();
        let signature = tx.signed_data.tx_signature;

        let tx_digest = tx1.digest();
        let sui_event = SuiEvent::TransferObject {
            package_id: ObjectID::from_hex_literal("0x2").unwrap(),
            transaction_module: String::from("native"),
            sender: signer,
            recipient: Owner::AddressOwner(recipient),
            object_id: object_ref.0,
            version: object_ref.1,
            type_: TransferType::ToAddress,
            amount: Some(100),
        };
        let events = vec![SuiEventEnvelope {
            timestamp: std::time::Instant::now().elapsed().as_secs(),
            tx_digest: Some(*tx_digest),
            event: sui_event.clone(),
        }];
        let result = SuiTransactionResponse {
            certificate: SuiCertifiedTransaction {
                transaction_digest: *tx_digest,
                data: SuiTransactionData::try_from(data1).unwrap(),
                tx_signature: signature.clone(),
                auth_sign_info: AuthorityQuorumSignInfo {
                    epoch: 0,
                    signature: Default::default(),
                    signers_map: Default::default(),
                },
            },
            effects: SuiTransactionEffects {
                status: SuiExecutionStatus::Success,
                gas_used: SuiGasCostSummary {
                    computation_cost: 100,
                    storage_cost: 100,
                    storage_rebate: 10,
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
                wrapped: vec![],
                gas_object: OwnedObjectRef {
                    owner: Owner::ObjectOwner(signer),
                    reference: SuiObjectRef::from(gas_ref),
                },
                events: vec![sui_event],
                dependencies: vec![],
            },
            timestamp_ms: None,
            parsed_data: None,
        };

        (
            data2,
            tx.signed_data.intent,
            signature,
            recipient,
            obj_id,
            result,
            events,
        )
    }

    fn get_events_by_transaction(&mut self) -> Examples {
        let (_, _, _, _, _, result, events) = self.get_transfer_data_response();
        Examples::new(
            "sui_getEventsByTransaction",
            vec![ExamplePairing::new(
                "Return the Events emitted by a transaction",
                vec![
                    (
                        "digest",
                        json!(result.certificate.transaction_digest.clone()),
                    ),
                    ("count", json!(2)),
                ],
                json!(events),
            )],
        )
    }

    fn get_events_by_sender(&mut self) -> Examples {
        let ts = std::time::Instant::now().elapsed().as_secs();
        let (tx_data, _, _, _, _, _, events) = self.get_transfer_data_response();
        Examples::new(
            "sui_getEventsBySender",
            vec![ExamplePairing::new(
                "Return the Events associated with the given sender",
                vec![
                    ("sender", json!(tx_data.signer())),
                    ("count", json!(2)),
                    ("start_time", json!(ts)),
                    ("end_time", json!(ts + 10)),
                ],
                json!(events),
            )],
        )
    }

    fn get_events_by_recipient(&mut self) -> Examples {
        let ts = std::time::Instant::now().elapsed().as_secs();
        let (_, _, _, recipient, _, _, events) = self.get_transfer_data_response();
        Examples::new(
            "sui_getEventsByRecipient",
            vec![ExamplePairing::new(
                "Return the Events associated with the given recipient",
                vec![
                    ("recipient", json!(Owner::AddressOwner(recipient))),
                    ("count", json!(2)),
                    ("start_time", json!(ts)),
                    ("end_time", json!(ts + 10)),
                ],
                json!(events),
            )],
        )
    }

    fn get_events_by_object(&mut self) -> Examples {
        let ts = std::time::Instant::now().elapsed().as_secs();
        let (_, _, _, _, obj_id, _, events) = self.get_transfer_data_response();
        Examples::new(
            "sui_getEventsByObject",
            vec![ExamplePairing::new(
                "Return the Events associated with the given object",
                vec![
                    ("object", json!(obj_id)),
                    ("count", json!(2)),
                    ("start_time", json!(ts)),
                    ("end_time", json!(ts + 10)),
                ],
                json!(events),
            )],
        )
    }

    fn get_events_by_timerange(&mut self) -> Examples {
        let ts = std::time::Instant::now().elapsed().as_secs();
        let (_, _, _, _, _, _, events) = self.get_transfer_data_response();
        Examples::new(
            "sui_getEventsByTimeRange",
            vec![ExamplePairing::new(
                "Return the Events emitted in [start_time, end_time) interval",
                vec![
                    ("count", json!(2)),
                    ("start_time", json!(ts)),
                    ("end_time", json!(ts + 10)),
                ],
                json!(events),
            )],
        )
    }

    fn get_events_by_move_event_struct_name(&mut self) -> Examples {
        let ts = std::time::Instant::now().elapsed().as_secs();
        let (data, intent, signature, _, _, _, _) = self.get_transfer_data_response();
        let tx = Transaction::new(data, intent, signature);

        let event = SuiEventEnvelope {
            timestamp: ts,
            tx_digest: Some(*tx.digest()),
            event: SuiEvent::MoveEvent {
                package_id: ObjectID::from_hex_literal("0x2").unwrap(),
                transaction_module: String::from("devnet_nft"),
                sender: SuiAddress::from_str("0x9421e7ad826ba13aca8ae41316644f06759b4506").unwrap(),
                type_: String::from("0x2::devnet_nft::MintNFTEvent"),
                fields: None,
                bcs: vec![],
            },
        };
        Examples::new(
            "sui_getEventsByMoveEventStructName",
            vec![ExamplePairing::new(
                "Return the Events with the given move event struct name",
                vec![
                    (
                        "move_event_struct_name",
                        json!("0x2::devnet_nft::MintNFTEvent"),
                    ),
                    ("count", json!(5)),
                    ("start_time", json!(ts)),
                    ("end_time", json!(ts + 10)),
                ],
                json!(vec![event]),
            )],
        )
    }

    fn get_events_by_transaction_module(&mut self) -> Examples {
        let ts = std::time::Instant::now().elapsed().as_secs();
        let (_, _, _, _, _, _, events) = self.get_transfer_data_response();
        Examples::new(
            "sui_getEventsByModule",
            vec![ExamplePairing::new(
                "Return the Events emitted in a specified Move module",
                vec![
                    ("package", json!(ObjectID::from_hex_literal("0x2").unwrap())),
                    ("module", json!("devnet_nft")),
                    ("count", json!(5)),
                    ("start_time", json!(ts)),
                    ("end_time", json!(ts + 10)),
                ],
                json!(events),
            )],
        )
    }
}
