// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::ops::Range;
use std::str::FromStr;

use move_core_types::identifier::Identifier;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use serde_json::json;

use sui::client_commands::EXAMPLE_NFT_DESCRIPTION;
use sui::client_commands::EXAMPLE_NFT_NAME;
use sui::client_commands::EXAMPLE_NFT_URL;
use sui_json::SuiJsonValue;
use sui_json_rpc_types::{
    GatewayTxSeqNumber, MoveCallParams, OwnedObjectRef, RPCTransactionRequestParams,
    SuiCertifiedTransaction, SuiData, SuiExecutionStatus, SuiGasCostSummary, SuiMoveObject,
    SuiObject, SuiObjectRead, SuiObjectRef, SuiParsedMoveObject, SuiRawMoveObject,
    SuiTransactionData, SuiTransactionEffects, TransactionBytes, TransactionEffectsResponse,
    TransactionResponse, TransferObjectParams,
};
use sui_open_rpc::ExamplePairing;
use sui_types::base_types::{
    ObjectDigest, ObjectID, ObjectInfo, SequenceNumber, SuiAddress, TransactionDigest,
};
use sui_types::crypto::{get_key_pair_from_rng, AccountKeyPair, Signature};
use sui_types::crypto::{AuthorityQuorumSignInfo, SuiSignature};
use sui_types::gas_coin::GasCoin;
use sui_types::messages::{
    CallArg, MoveCall, SingleTransactionKind, TransactionData, TransactionKind, TransferObject,
};
use sui_types::object::Owner;
use sui_types::sui_serde::Base64;
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
            self.get_objects_owned_by_address(),
            self.get_objects_owned_by_object(),
            self.get_raw_object(),
            self.get_recent_transactions(),
            self.get_total_transaction_number(),
            self.get_transaction(),
            self.get_transactions_by_input_object(),
            self.get_transactions_by_move_function(),
            self.get_transactions_by_mutated_object(),
            self.get_transactions_from_address(),
            self.get_transactions_in_range(),
            self.get_transactions_to_address(),
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
        let (data, signature, result) = self.get_transfer_data_response();
        let tx_bytes = TransactionBytes::from_data(data).unwrap();

        Examples::new(
            "sui_executeTransaction",
            vec![ExamplePairing::new(
                "Execute an object transfer transaction",
                vec![
                    ("tx_bytes", json!(tx_bytes.tx_bytes)),
                    ("sig_scheme", json!(signature.scheme())),
                    (
                        "signature",
                        json!(Base64::from_bytes(signature.signature_bytes())),
                    ),
                    (
                        "pub_key",
                        json!(Base64::from_bytes(signature.public_key_bytes())),
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
            data: SuiData::MoveObject(
                SuiParsedMoveObject::try_from_layout(
                    coin.to_object(SequenceNumber::from_u64(1)),
                    GasCoin::layout(),
                )
                .unwrap(),
            ),
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
            data: SuiData::MoveObject(SuiRawMoveObject {
                type_: GasCoin::type_().to_string(),
                has_public_transfer: object.has_public_transfer(),
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
        let (_, _, result) = self.get_transfer_data_response();
        let result = result.to_effect_response().unwrap();
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

    fn get_transactions_by_input_object(&mut self) -> Examples {
        let result = self.get_transaction_digests(5..8);
        Examples::new(
            "sui_getTransactionsByInputObject",
            vec![ExamplePairing::new(
                "Return the transaction digest for specified input object",
                vec![("object", json!(ObjectID::new(self.rng.gen())))],
                json!(result),
            )],
        )
    }

    fn get_transactions_by_move_function(&mut self) -> Examples {
        let result = self.get_transaction_digests(6..10);
        Examples::new(
            "sui_getTransactionsByMoveFunction",
            vec![ExamplePairing::new(
                "Return the transaction digest for specified input object",
                vec![
                    ("package", json!(SUI_FRAMEWORK_OBJECT_ID)),
                    ("module", json!("devnet_nft")),
                    ("function", json!("function")),
                ],
                json!(result),
            )],
        )
    }

    fn get_transactions_by_mutated_object(&mut self) -> Examples {
        let result = self.get_transaction_digests(5..8);
        Examples::new(
            "sui_getTransactionsByMutatedObject",
            vec![ExamplePairing::new(
                "Return the transaction digest for specified mutated object",
                vec![("object", json!(ObjectID::new(self.rng.gen())))],
                json!(result),
            )],
        )
    }

    fn get_transactions_from_address(&mut self) -> Examples {
        let result = self.get_transaction_digests(5..8);
        Examples::new(
            "sui_getTransactionsFromAddress",
            vec![ExamplePairing::new(
                "Return the transaction digest for specified sender address",
                vec![(
                    "addr",
                    json!(SuiAddress::from(ObjectID::new(self.rng.gen()))),
                )],
                json!(result),
            )],
        )
    }

    fn get_transactions_in_range(&mut self) -> Examples {
        let result = self.get_transaction_digests(5..8);
        Examples::new(
            "sui_getTransactionsInRange",
            vec![ExamplePairing::new(
                "Return the transaction digest in range",
                vec![("start", json!(5)), ("end", json!(8))],
                json!(result),
            )],
        )
    }

    fn get_transactions_to_address(&mut self) -> Examples {
        let result = self.get_transaction_digests(5..8);
        Examples::new(
            "sui_getTransactionsToAddress",
            vec![ExamplePairing::new(
                "Return the transaction digest for specified recipient address",
                vec![(
                    "addr",
                    json!(SuiAddress::from(ObjectID::new(self.rng.gen()))),
                )],
                json!(result),
            )],
        )
    }

    fn get_transaction_digests(
        &mut self,
        range: Range<u64>,
    ) -> Vec<(GatewayTxSeqNumber, TransactionDigest)> {
        range
            .into_iter()
            .map(|seq| (seq, TransactionDigest::new(self.rng.gen())))
            .collect()
    }

    fn get_transfer_data_response(&mut self) -> (TransactionData, Signature, TransactionResponse) {
        let (signer, kp): (_, AccountKeyPair) = get_key_pair_from_rng(&mut self.rng);
        let recipient = SuiAddress::from(ObjectID::new(self.rng.gen()));
        let gas_ref = (
            ObjectID::new(self.rng.gen()),
            SequenceNumber::from_u64(2),
            ObjectDigest::new(self.rng.gen()),
        );
        let object_ref = (
            ObjectID::new(self.rng.gen()),
            SequenceNumber::from_u64(2),
            ObjectDigest::new(self.rng.gen()),
        );

        let data = TransactionData::new_transfer(recipient, object_ref, signer, gas_ref, 1000);
        let signature = Signature::new(&data, &kp);

        let result = TransactionResponse::EffectResponse(TransactionEffectsResponse {
            certificate: SuiCertifiedTransaction {
                transaction_digest: TransactionDigest::new(self.rng.gen()),
                data: SuiTransactionData::try_from(data.clone()).unwrap(),
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
                events: vec![],
                dependencies: vec![],
            },
            timestamp_ms: None,
        });

        (data, signature, result)
    }
}
