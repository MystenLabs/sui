// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::ident_str;
use shared_crypto::intent::Intent;
use std::path::PathBuf;
use sui_genesis_builder::validator_info::GenesisValidatorMetadata;
use sui_move_build::BuildConfig;
use sui_sdk::rpc_types::{
    get_new_package_obj_from_response, SuiTransactionBlockEffectsAPI, SuiTransactionBlockResponse,
};
use sui_sdk::wallet_context::WalletContext;
use sui_types::base_types::{ObjectID, ObjectRef, SequenceNumber, SuiAddress};
use sui_types::crypto::{get_key_pair, AccountKeyPair, Signature, Signer};
use sui_types::digests::TransactionDigest;
use sui_types::object::Owner;
use sui_types::sui_system_state::SUI_SYSTEM_MODULE_NAME;
use sui_types::transaction::{
    CallArg, ObjectArg, ProgrammableTransaction, Transaction, TransactionData,
    DEFAULT_VALIDATOR_GAS_PRICE, TEST_ONLY_GAS_UNIT_FOR_GENERIC, TEST_ONLY_GAS_UNIT_FOR_TRANSFER,
};
use sui_types::{TypeTag, SUI_SYSTEM_PACKAGE_ID};

pub struct TestTransactionBuilder {
    test_data: TestTransactionData,
    sender: SuiAddress,
    gas_object: ObjectRef,
    gas_price: u64,
}

impl TestTransactionBuilder {
    pub fn new(sender: SuiAddress, gas_object: ObjectRef, gas_price: u64) -> Self {
        Self {
            test_data: TestTransactionData::Empty,
            sender,
            gas_object,
            gas_price,
        }
    }

    pub fn move_call(
        mut self,
        package_id: ObjectID,
        module: &'static str,
        function: &'static str,
        args: Vec<CallArg>,
    ) -> Self {
        assert!(matches!(self.test_data, TestTransactionData::Empty));
        self.test_data = TestTransactionData::Move(MoveData {
            package_id,
            module,
            function,
            args,
            type_args: vec![],
        });
        self
    }

    pub fn call_counter_create(self, package_id: ObjectID) -> Self {
        self.move_call(package_id, "counter", "create", vec![])
    }

    pub fn call_counter_increment(
        self,
        package_id: ObjectID,
        counter_id: ObjectID,
        counter_initial_shared_version: SequenceNumber,
    ) -> Self {
        self.move_call(
            package_id,
            "counter",
            "increment",
            vec![CallArg::Object(ObjectArg::SharedObject {
                id: counter_id,
                initial_shared_version: counter_initial_shared_version,
                mutable: true,
            })],
        )
    }

    pub fn call_nft_create(self, package_id: ObjectID) -> Self {
        self.move_call(
            package_id,
            "devnet_nft",
            "mint",
            vec![
                CallArg::Pure(bcs::to_bytes("example_nft_name").unwrap()),
                CallArg::Pure(bcs::to_bytes("example_nft_description").unwrap()),
                CallArg::Pure(
                    bcs::to_bytes("https://sui.io/_nuxt/img/sui-logo.8d3c44e.svg").unwrap(),
                ),
            ],
        )
    }

    pub fn call_nft_delete(self, package_id: ObjectID, nft_to_delete: ObjectRef) -> Self {
        self.move_call(
            package_id,
            "devnet_nft",
            "burn",
            vec![CallArg::Object(ObjectArg::ImmOrOwnedObject(nft_to_delete))],
        )
    }

    pub fn call_staking(self, stake_coin: ObjectRef, validator: SuiAddress) -> Self {
        self.move_call(
            SUI_SYSTEM_PACKAGE_ID,
            SUI_SYSTEM_MODULE_NAME.as_str(),
            "request_add_stake",
            vec![
                CallArg::SUI_SYSTEM_MUT,
                CallArg::Object(ObjectArg::ImmOrOwnedObject(stake_coin)),
                CallArg::Pure(bcs::to_bytes(&validator).unwrap()),
            ],
        )
    }

    pub fn call_request_add_validator(self) -> Self {
        self.move_call(
            SUI_SYSTEM_PACKAGE_ID,
            SUI_SYSTEM_MODULE_NAME.as_str(),
            "request_add_validator",
            vec![CallArg::SUI_SYSTEM_MUT],
        )
    }

    pub fn call_request_add_validator_candidate(
        self,
        validator: &GenesisValidatorMetadata,
    ) -> Self {
        self.move_call(
            SUI_SYSTEM_PACKAGE_ID,
            SUI_SYSTEM_MODULE_NAME.as_str(),
            "request_add_validator_candidate",
            vec![
                CallArg::SUI_SYSTEM_MUT,
                CallArg::Pure(bcs::to_bytes(&validator.protocol_public_key).unwrap()),
                CallArg::Pure(bcs::to_bytes(&validator.network_public_key).unwrap()),
                CallArg::Pure(bcs::to_bytes(&validator.worker_public_key).unwrap()),
                CallArg::Pure(bcs::to_bytes(&validator.proof_of_possession).unwrap()),
                CallArg::Pure(bcs::to_bytes(validator.name.as_bytes()).unwrap()),
                CallArg::Pure(bcs::to_bytes(validator.description.as_bytes()).unwrap()),
                CallArg::Pure(bcs::to_bytes(validator.image_url.as_bytes()).unwrap()),
                CallArg::Pure(bcs::to_bytes(validator.project_url.as_bytes()).unwrap()),
                CallArg::Pure(bcs::to_bytes(&validator.network_address).unwrap()),
                CallArg::Pure(bcs::to_bytes(&validator.p2p_address).unwrap()),
                CallArg::Pure(bcs::to_bytes(&validator.primary_address).unwrap()),
                CallArg::Pure(bcs::to_bytes(&validator.worker_address).unwrap()),
                CallArg::Pure(bcs::to_bytes(&DEFAULT_VALIDATOR_GAS_PRICE).unwrap()), // gas_price
                CallArg::Pure(bcs::to_bytes(&0u64).unwrap()), // commission_rate
            ],
        )
    }

    pub fn call_request_remove_validator(self) -> Self {
        self.move_call(
            SUI_SYSTEM_PACKAGE_ID,
            SUI_SYSTEM_MODULE_NAME.as_str(),
            "request_remove_validator",
            vec![CallArg::SUI_SYSTEM_MUT],
        )
    }

    pub fn with_type_args(mut self, type_args: Vec<TypeTag>) -> Self {
        if let TestTransactionData::Move(data) = &mut self.test_data {
            assert!(data.type_args.is_empty());
            data.type_args = type_args;
        } else {
            panic!("Cannot set type args for non-move call");
        }
        self
    }

    pub fn transfer(mut self, object: ObjectRef, recipient: SuiAddress) -> Self {
        self.test_data = TestTransactionData::Transfer(TransferData { object, recipient });
        self
    }

    pub fn transfer_sui(mut self, amount: Option<u64>, recipient: SuiAddress) -> Self {
        self.test_data = TestTransactionData::TransferSui(TransferSuiData { amount, recipient });
        self
    }

    pub fn publish(mut self, path: PathBuf) -> Self {
        assert!(matches!(self.test_data, TestTransactionData::Empty));
        self.test_data = TestTransactionData::Publish(PublishData {
            path,
            with_unpublished_deps: false,
        });
        self
    }

    pub fn publish_with_deps(mut self, path: PathBuf) -> Self {
        assert!(matches!(self.test_data, TestTransactionData::Empty));
        self.test_data = TestTransactionData::Publish(PublishData {
            path,
            with_unpublished_deps: true,
        });
        self
    }

    pub fn publish_examples(self, subpath: &'static str) -> Self {
        let path = if let Ok(p) = std::env::var("MOVE_EXAMPLES_DIR") {
            let mut path = PathBuf::from(p);
            path.extend([subpath]);
            path
        } else {
            let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            path.extend(["..", "..", "sui_programmability", "examples", subpath]);
            path
        };
        self.publish(path)
    }

    pub fn programmable(mut self, programmable: ProgrammableTransaction) -> Self {
        self.test_data = TestTransactionData::Programmable(programmable);
        self
    }

    pub fn build(self) -> TransactionData {
        match self.test_data {
            TestTransactionData::Move(data) => TransactionData::new_move_call(
                self.sender,
                data.package_id,
                ident_str!(data.module).to_owned(),
                ident_str!(data.function).to_owned(),
                data.type_args,
                self.gas_object,
                data.args,
                self.gas_price * TEST_ONLY_GAS_UNIT_FOR_GENERIC,
                self.gas_price,
            )
            .unwrap(),
            TestTransactionData::Transfer(data) => TransactionData::new_transfer(
                data.recipient,
                data.object,
                self.sender,
                self.gas_object,
                self.gas_price * TEST_ONLY_GAS_UNIT_FOR_TRANSFER,
                self.gas_price,
            ),
            TestTransactionData::TransferSui(data) => TransactionData::new_transfer_sui(
                data.recipient,
                self.sender,
                data.amount,
                self.gas_object,
                self.gas_price * TEST_ONLY_GAS_UNIT_FOR_TRANSFER,
                self.gas_price,
            ),
            TestTransactionData::Publish(data) => {
                let compiled_package = BuildConfig::new_for_testing().build(data.path).unwrap();
                let all_module_bytes =
                    compiled_package.get_package_bytes(data.with_unpublished_deps);
                let dependencies = compiled_package.get_dependency_original_package_ids();

                TransactionData::new_module(
                    self.sender,
                    self.gas_object,
                    all_module_bytes,
                    dependencies,
                    self.gas_price * TEST_ONLY_GAS_UNIT_FOR_GENERIC,
                    self.gas_price,
                )
            }
            TestTransactionData::Programmable(pt) => TransactionData::new_programmable(
                self.sender,
                vec![self.gas_object],
                pt,
                self.gas_price * TEST_ONLY_GAS_UNIT_FOR_GENERIC,
                self.gas_price,
            ),
            TestTransactionData::Empty => {
                panic!("Cannot build empty transaction");
            }
        }
    }

    pub fn build_and_sign(self, signer: &dyn Signer<Signature>) -> Transaction {
        Transaction::from_data_and_signer(self.build(), Intent::sui_transaction(), vec![signer])
    }
}

enum TestTransactionData {
    Move(MoveData),
    Transfer(TransferData),
    TransferSui(TransferSuiData),
    Publish(PublishData),
    Programmable(ProgrammableTransaction),
    Empty,
}

struct MoveData {
    package_id: ObjectID,
    module: &'static str,
    function: &'static str,
    args: Vec<CallArg>,
    type_args: Vec<TypeTag>,
}

struct PublishData {
    path: PathBuf,
    /// Whether to publish unpublished dependencies in the same transaction or not.
    with_unpublished_deps: bool,
}

struct TransferData {
    object: ObjectRef,
    recipient: SuiAddress,
}

struct TransferSuiData {
    amount: Option<u64>,
    recipient: SuiAddress,
}

/// A helper function to make Transactions with controlled accounts in WalletContext.
/// Particularly, the wallet needs to own gas objects for transactions.
/// However, if this function is called multiple times without any "sync" actions
/// on gas object management, txns may fail and objects may be locked.
///
/// The param is called `max_txn_num` because it does not always return the exact
/// same amount of Transactions, for example when there are not enough gas objects
/// controlled by the WalletContext. Caller should rely on the return value to
/// check the count.
pub async fn batch_make_transfer_transactions(
    context: &WalletContext,
    max_txn_num: usize,
) -> Vec<Transaction> {
    let recipient = get_key_pair::<AccountKeyPair>().0;
    let accounts_and_objs = context.get_all_accounts_and_gas_objects().await.unwrap();
    let mut res = Vec::with_capacity(max_txn_num);

    let gas_price = context.get_reference_gas_price().await.unwrap();
    for (address, objs) in accounts_and_objs {
        for obj in objs {
            if res.len() >= max_txn_num {
                return res;
            }
            let data = TransactionData::new_transfer_sui(
                recipient,
                address,
                Some(2),
                obj,
                gas_price * TEST_ONLY_GAS_UNIT_FOR_TRANSFER,
                gas_price,
            );
            let tx = context.sign_transaction(&data);
            res.push(tx);
        }
    }
    res
}

pub async fn make_transfer_sui_transaction(
    context: &WalletContext,
    recipient: Option<SuiAddress>,
    amount: Option<u64>,
) -> Transaction {
    let (sender, gas_object) = context.get_one_gas_object().await.unwrap().unwrap();
    let gas_price = context.get_reference_gas_price().await.unwrap();
    context.sign_transaction(
        &TestTransactionBuilder::new(sender, gas_object, gas_price)
            .transfer_sui(amount, recipient.unwrap_or(sender))
            .build(),
    )
}

pub async fn make_staking_transaction(
    context: &WalletContext,
    validator_address: SuiAddress,
) -> Transaction {
    let accounts_and_objs = context.get_all_accounts_and_gas_objects().await.unwrap();
    let sender = accounts_and_objs[0].0;
    let gas_object = accounts_and_objs[0].1[0];
    let stake_object = accounts_and_objs[0].1[1];
    let gas_price = context.get_reference_gas_price().await.unwrap();
    context.sign_transaction(
        &TestTransactionBuilder::new(sender, gas_object, gas_price)
            .call_staking(stake_object, validator_address)
            .build(),
    )
}

pub async fn make_publish_transaction(context: &WalletContext, path: PathBuf) -> Transaction {
    let (sender, gas_object) = context.get_one_gas_object().await.unwrap().unwrap();
    let gas_price = context.get_reference_gas_price().await.unwrap();
    context.sign_transaction(
        &TestTransactionBuilder::new(sender, gas_object, gas_price)
            .publish(path)
            .build(),
    )
}

pub async fn make_publish_transaction_with_deps(
    context: &WalletContext,
    path: PathBuf,
) -> Transaction {
    let (sender, gas_object) = context.get_one_gas_object().await.unwrap().unwrap();
    let gas_price = context.get_reference_gas_price().await.unwrap();
    context.sign_transaction(
        &TestTransactionBuilder::new(sender, gas_object, gas_price)
            .publish_with_deps(path)
            .build(),
    )
}

pub async fn publish_package(context: &WalletContext, path: PathBuf) -> ObjectRef {
    let (sender, gas_object) = context.get_one_gas_object().await.unwrap().unwrap();
    let gas_price = context.get_reference_gas_price().await.unwrap();
    let txn = context.sign_transaction(
        &TestTransactionBuilder::new(sender, gas_object, gas_price)
            .publish(path)
            .build(),
    );
    let resp = context.execute_transaction_must_succeed(txn).await;
    get_new_package_obj_from_response(&resp).unwrap()
}

/// Executes a transaction to publish the `basics` package and returns the package object ref.
pub async fn publish_basics_package(context: &WalletContext) -> ObjectRef {
    let (sender, gas_object) = context.get_one_gas_object().await.unwrap().unwrap();
    let gas_price = context.get_reference_gas_price().await.unwrap();
    let txn = context.sign_transaction(
        &TestTransactionBuilder::new(sender, gas_object, gas_price)
            .publish_examples("basics")
            .build(),
    );
    let resp = context.execute_transaction_must_succeed(txn).await;
    get_new_package_obj_from_response(&resp).unwrap()
}

/// Executes a transaction to publish the `basics` package and another one to create a counter.
/// Returns the package object ref and the counter object ref.
pub async fn publish_basics_package_and_make_counter(
    context: &WalletContext,
) -> (ObjectRef, ObjectRef) {
    let package_ref = publish_basics_package(context).await;
    let (sender, gas_object) = context.get_one_gas_object().await.unwrap().unwrap();
    let gas_price = context.get_reference_gas_price().await.unwrap();
    let counter_creation_txn = context.sign_transaction(
        &TestTransactionBuilder::new(sender, gas_object, gas_price)
            .call_counter_create(package_ref.0)
            .build(),
    );
    let resp = context
        .execute_transaction_must_succeed(counter_creation_txn)
        .await;
    let counter_ref = resp
        .effects
        .unwrap()
        .created()
        .iter()
        .find(|obj_ref| matches!(obj_ref.owner, Owner::Shared { .. }))
        .unwrap()
        .reference
        .to_object_ref();
    (package_ref, counter_ref)
}

/// Executes a transaction to increment a counter object.
/// Must be called after calling `publish_basics_package_and_make_counter`.
pub async fn increment_counter(
    context: &WalletContext,
    sender: SuiAddress,
    gas_object_id: Option<ObjectID>,
    package_id: ObjectID,
    counter_id: ObjectID,
    initial_shared_version: SequenceNumber,
) -> SuiTransactionBlockResponse {
    let gas_object = if let Some(gas_object_id) = gas_object_id {
        context.get_object_ref(gas_object_id).await.unwrap()
    } else {
        context
            .get_one_gas_object_owned_by_address(sender)
            .await
            .unwrap()
            .unwrap()
    };
    let rgp = context.get_reference_gas_price().await.unwrap();
    let txn = context.sign_transaction(
        &TestTransactionBuilder::new(sender, gas_object, rgp)
            .call_counter_increment(package_id, counter_id, initial_shared_version)
            .build(),
    );
    context.execute_transaction_must_succeed(txn).await
}

/// Executes a transaction to publish the `nfts` package and returns the package id, id of the gas
/// object used, and the digest of the transaction.
pub async fn publish_nfts_package(
    context: &WalletContext,
) -> (ObjectID, ObjectID, TransactionDigest) {
    let (sender, gas_object) = context.get_one_gas_object().await.unwrap().unwrap();
    let gas_id = gas_object.0;
    let gas_price = context.get_reference_gas_price().await.unwrap();
    let txn = context.sign_transaction(
        &TestTransactionBuilder::new(sender, gas_object, gas_price)
            .publish_examples("nfts")
            .build(),
    );
    let resp = context.execute_transaction_must_succeed(txn).await;
    let package_id = get_new_package_obj_from_response(&resp).unwrap().0;
    (package_id, gas_id, resp.digest)
}

/// Pre-requisite: `publish_nfts_package` must be called before this function.  Executes a
/// transaction to create an NFT and returns the sender address, the object id of the NFT, and the
/// digest of the transaction.
pub async fn create_devnet_nft(
    context: &WalletContext,
    package_id: ObjectID,
) -> (SuiAddress, ObjectID, TransactionDigest) {
    let (sender, gas_object) = context.get_one_gas_object().await.unwrap().unwrap();
    let rgp = context.get_reference_gas_price().await.unwrap();

    let txn = context.sign_transaction(
        &TestTransactionBuilder::new(sender, gas_object, rgp)
            .call_nft_create(package_id)
            .build(),
    );
    let resp = context.execute_transaction_must_succeed(txn).await;

    let object_id = resp
        .effects
        .as_ref()
        .unwrap()
        .created()
        .first()
        .unwrap()
        .reference
        .object_id;

    (sender, object_id, resp.digest)
}

/// Executes a transaction to delete the given NFT.
pub async fn delete_devnet_nft(
    context: &WalletContext,
    sender: SuiAddress,
    package_id: ObjectID,
    nft_to_delete: ObjectRef,
) -> SuiTransactionBlockResponse {
    let gas = context
        .get_one_gas_object_owned_by_address(sender)
        .await
        .unwrap()
        .unwrap_or_else(|| panic!("Expect {sender} to have at least one gas object"));
    let rgp = context.get_reference_gas_price().await.unwrap();
    let txn = context.sign_transaction(
        &TestTransactionBuilder::new(sender, gas, rgp)
            .call_nft_delete(package_id, nft_to_delete)
            .build(),
    );
    context.execute_transaction_must_succeed(txn).await
}
