// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::ops::Range;
use std::str::FromStr;

use fastcrypto::traits::AggregateAuthenticator;
use fastcrypto::traits::EncodeDecodeBase64;
use fastcrypto::traits::KeyPair;
use move_core_types::identifier::Identifier;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use serde_json::json;
use sui::client_commands::EXAMPLE_NFT_DESCRIPTION;
use sui::client_commands::EXAMPLE_NFT_NAME;
use sui::client_commands::EXAMPLE_NFT_URL;
use sui_json::SuiJsonValue;
use sui_json_rpc_types::{
    EventPage, MoveCallParams, OwnedObjectRef, RPCTransactionRequestParams,
    SuiCertifiedTransaction, SuiData, SuiEvent, SuiEventEnvelope, SuiExecutionStatus,
    SuiGasCostSummary, SuiObject, SuiObjectInfo, SuiObjectRead, SuiObjectRef, SuiParsedData,
    SuiPastObjectRead, SuiRawData, SuiRawMoveObject, SuiTransactionAuthSignersResponse,
    SuiTransactionData, SuiTransactionEffects, SuiTransactionResponse, TransactionBytes,
    TransactionsPage, TransferObjectParams,
};
use sui_open_rpc::ExamplePairing;
use sui_types::base_types::{
    ObjectDigest, ObjectID, ObjectType, SequenceNumber, SuiAddress, TransactionDigest,
};
use sui_types::crypto::AuthorityQuorumSignInfo;
use sui_types::crypto::{
    get_key_pair_from_rng, AccountKeyPair, AggregateAuthoritySignature, AuthorityKeyPair,
    AuthorityPublicKeyBytes, AuthoritySignature, SuiAuthorityStrongQuorumSignInfo,
};
use sui_types::event::EventID;
use sui_types::gas_coin::GasCoin;
use sui_types::messages::{
    CallArg, ExecuteTransactionRequestType, MoveCall, SingleTransactionKind, TransactionData,
    TransactionKind, TransferObject,
};
use sui_types::multisig::GenericSignature;
use sui_types::object::Owner;
use sui_types::query::EventQuery;
use sui_types::query::TransactionQuery;
use sui_types::utils::to_sender_signed_transaction;
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
            self.get_object_example(),
            self.get_past_object_example(),
            self.get_objects_owned_by_address(),
            self.get_objects_owned_by_object(),
            self.get_raw_object(),
            self.get_total_transaction_number(),
            self.get_transaction(),
            self.get_transaction_auth_signers(),
            self.get_transactions(),
            self.get_events(),
            self.execute_transaction_example(),
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

        let data = TransactionData::new_with_dummy_gas_price(
            TransactionKind::Batch(vec![
                SingleTransactionKind::Call(MoveCall {
                    package: SUI_FRAMEWORK_OBJECT_ID,
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
                    ("txn_builder_mode", json!("Commit")),
                ],
                json!(result),
            )],
        )
    }

    fn execute_transaction_example(&mut self) -> Examples {
        let (data, signature, _, _, result, _) = self.get_transfer_data_response();
        let tx_bytes = TransactionBytes::from_data(data).unwrap();

        Examples::new(
            "sui_executeTransaction",
            vec![ExamplePairing::new(
                "Execute an transaction with serialized signature",
                vec![
                    ("tx_bytes", json!(tx_bytes.tx_bytes)),
                    ("signature", json!(signature.encode_base64())),
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
            .map(|_| SuiObjectInfo {
                object_id: ObjectID::new(self.rng.gen()),
                version: Default::default(),
                digest: ObjectDigest::new(self.rng.gen()),
                type_: ObjectType::Struct(GasCoin::type_()).to_string(),
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
            .map(|_| SuiObjectInfo {
                object_id: ObjectID::new(self.rng.gen()),
                version: Default::default(),
                digest: ObjectDigest::new(self.rng.gen()),
                type_: ObjectType::Struct(GasCoin::type_()).to_string(),
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
        let (_, _, _, _, result, _) = self.get_transfer_data_response();
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

    fn get_transaction_auth_signers(&mut self) -> Examples {
        let (_, _, _, _, tx_result, _) = self.get_transfer_data_response();
        let sec1: AuthorityKeyPair = get_key_pair_from_rng(&mut StdRng::from_seed([0; 32])).1;
        let sec2: AuthorityKeyPair = get_key_pair_from_rng(&mut StdRng::from_seed([1; 32])).1;
        let sec3: AuthorityKeyPair = get_key_pair_from_rng(&mut StdRng::from_seed([2; 32])).1;

        let result = SuiTransactionAuthSignersResponse {
            signers: vec![
                AuthorityPublicKeyBytes::from(sec1.public()),
                AuthorityPublicKeyBytes::from(sec2.public()),
                AuthorityPublicKeyBytes::from(sec3.public()),
            ],
        };
        Examples::new(
            "sui_getTransactionAuthSigners",
            vec![ExamplePairing::new(
                "Return the list of authorities that committed to the authority signature of the specified transaction digest",
                vec![(
                    "digest",
                    json!(tx_result.certificate.transaction_digest.clone()),
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
        GenericSignature,
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

        let data = TransactionData::new_transfer_with_dummy_gas_price(
            recipient, object_ref, signer, gas_ref, 1000,
        );
        let data1 = data.clone();
        let data2 = data.clone();

        let tx = to_sender_signed_transaction(data, &kp);
        let tx1 = tx.clone();
        let signature = tx.into_inner().tx_signature.clone();

        let tx_digest = tx1.digest();
        let sui_event = SuiEvent::TransferObject {
            package_id: ObjectID::from_hex_literal("0x2").unwrap(),
            transaction_module: String::from("native"),
            sender: signer,
            recipient: Owner::AddressOwner(recipient),
            object_type: "0x2::example::Object".to_string(),
            object_id: object_ref.0,
            version: object_ref.1,
        };
        let events = vec![SuiEventEnvelope {
            timestamp: std::time::Instant::now().elapsed().as_secs(),
            tx_digest: *tx_digest,
            id: EventID::from((*tx_digest, 0)),
            event: sui_event.clone(),
        }];
        let result = SuiTransactionResponse {
            certificate: SuiCertifiedTransaction {
                transaction_digest: *tx_digest,
                data: SuiTransactionData::try_from(data1).unwrap(),
                tx_signature: signature.clone(),
                auth_sign_info: SuiAuthorityStrongQuorumSignInfo::from(&AuthorityQuorumSignInfo {
                    epoch: 0,
                    // We create a dummy signature since there is no such thing as a default valid
                    // signature.
                    signature: AggregateAuthoritySignature::aggregate(&vec![
                        AuthoritySignature::default(),
                    ])
                    .unwrap(),
                    signers_map: Default::default(),
                }),
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

        (data2, signature, recipient, obj_id, result, events)
    }

    fn get_events(&mut self) -> Examples {
        let (_, _, _, _, result, events) = self.get_transfer_data_response();
        let tx_dig =
            TransactionDigest::from_str("11a72GCQ5hGNpWGh2QhQkkusTEGS6EDqifJqxr7nSYX").unwrap();
        let page = EventPage {
            data: events.clone(),
            next_cursor: Some((tx_dig, 5).into()),
        };
        Examples::new(
            "sui_getEvents",
            vec![ExamplePairing::new(
                "Return the Events emitted by a transaction",
                vec![
                    (
                        "query",
                        json!(EventQuery::Transaction(
                            result.certificate.transaction_digest
                        )),
                    ),
                    (
                        "cursor",
                        json!(EventID {
                            event_seq: 10,
                            tx_digest: result.certificate.transaction_digest
                        }),
                    ),
                    ("limit", json!(events.len())),
                    ("descending_order", json!(false)),
                ],
                json!(page),
            )],
        )
    }
}
