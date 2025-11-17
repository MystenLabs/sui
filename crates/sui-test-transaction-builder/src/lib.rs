// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::ident_str;
use move_core_types::u256::U256;
use shared_crypto::intent::{Intent, IntentMessage};
use std::path::PathBuf;
use sui_genesis_builder::validator_info::GenesisValidatorMetadata;
use sui_move_build::{BuildConfig, CompiledPackage};
use sui_sdk::rpc_types::{
    SuiObjectDataOptions, SuiTransactionBlockEffectsAPI, SuiTransactionBlockResponse,
};
use sui_sdk::wallet_context::WalletContext;
use sui_types::balance::Balance;
use sui_types::base_types::{FullObjectRef, ObjectID, ObjectRef, SequenceNumber, SuiAddress};
use sui_types::crypto::{AccountKeyPair, Signature, Signer, get_key_pair};
use sui_types::digests::TransactionDigest;
use sui_types::gas_coin::GAS;
use sui_types::multisig::{BitmapUnit, MultiSig, MultiSigPublicKey};
use sui_types::multisig_legacy::{MultiSigLegacy, MultiSigPublicKeyLegacy};
use sui_types::object::Owner;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::signature::GenericSignature;
use sui_types::sui_system_state::SUI_SYSTEM_MODULE_NAME;
use sui_types::transaction::{
    Argument, CallArg, DEFAULT_VALIDATOR_GAS_PRICE, FundsWithdrawalArg, ObjectArg,
    ProgrammableTransaction, SharedObjectMutability,
    TEST_ONLY_GAS_UNIT_FOR_HEAVY_COMPUTATION_STORAGE, TEST_ONLY_GAS_UNIT_FOR_TRANSFER, Transaction,
    TransactionData,
};
use sui_types::{Identifier, SUI_FRAMEWORK_PACKAGE_ID, SUI_RANDOMNESS_STATE_OBJECT_ID};
use sui_types::{SUI_SYSTEM_PACKAGE_ID, TypeTag};

#[derive(Clone)]
pub enum FundSource {
    Coin(ObjectRef),
    AddressFund {
        /// If None, it will be set the same as the total transfer amount.
        reservation: Option<u64>,
    },
    // TODO: Add object fund source
}

impl FundSource {
    pub fn coin(coin: ObjectRef) -> Self {
        Self::Coin(coin)
    }

    pub fn address_fund() -> Self {
        Self::AddressFund { reservation: None }
    }

    pub fn address_fund_with_reservation(reservation: u64) -> Self {
        Self::AddressFund {
            reservation: Some(reservation),
        }
    }
}

pub struct TestTransactionBuilder {
    test_data: TestTransactionData,
    sender: SuiAddress,
    gas_object: ObjectRef,
    gas_price: u64,
    gas_budget: Option<u64>,
}

impl TestTransactionBuilder {
    pub fn new(sender: SuiAddress, gas_object: ObjectRef, gas_price: u64) -> Self {
        Self {
            test_data: TestTransactionData::Empty,
            sender,
            gas_object,
            gas_price,
            gas_budget: None,
        }
    }

    pub fn sender(&self) -> SuiAddress {
        self.sender
    }

    pub fn gas_object(&self) -> ObjectRef {
        self.gas_object
    }

    // Use `with_type_args` below to provide type args if any
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

    pub fn with_type_args(mut self, type_args: Vec<TypeTag>) -> Self {
        if let TestTransactionData::Move(data) = &mut self.test_data {
            assert!(data.type_args.is_empty());
            data.type_args = type_args;
        } else {
            panic!("Cannot set type args for non-move call");
        }
        self
    }

    pub fn with_gas_budget(mut self, gas_budget: u64) -> Self {
        self.gas_budget = Some(gas_budget);
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
                mutability: SharedObjectMutability::Mutable,
            })],
        )
    }

    pub fn call_counter_read(
        self,
        package_id: ObjectID,
        counter_id: ObjectID,
        counter_initial_shared_version: SequenceNumber,
    ) -> Self {
        self.move_call(
            package_id,
            "counter",
            "value",
            vec![CallArg::Object(ObjectArg::SharedObject {
                id: counter_id,
                initial_shared_version: counter_initial_shared_version,
                mutability: SharedObjectMutability::Immutable,
            })],
        )
    }

    pub fn call_counter_delete(
        self,
        package_id: ObjectID,
        counter_id: ObjectID,
        counter_initial_shared_version: SequenceNumber,
    ) -> Self {
        self.move_call(
            package_id,
            "counter",
            "delete",
            vec![CallArg::Object(ObjectArg::SharedObject {
                id: counter_id,
                initial_shared_version: counter_initial_shared_version,
                mutability: SharedObjectMutability::Mutable,
            })],
        )
    }

    pub fn call_nft_create(self, package_id: ObjectID) -> Self {
        self.move_call(
            package_id,
            "testnet_nft",
            "mint_to_sender",
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
            "testnet_nft",
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

    pub fn call_emit_random(
        self,
        package_id: ObjectID,
        randomness_initial_shared_version: SequenceNumber,
    ) -> Self {
        self.move_call(
            package_id,
            "random",
            "new",
            vec![CallArg::Object(ObjectArg::SharedObject {
                id: SUI_RANDOMNESS_STATE_OBJECT_ID,
                initial_shared_version: randomness_initial_shared_version,
                mutability: SharedObjectMutability::Immutable,
            })],
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

    pub fn call_object_create_party(self, package_id: ObjectID) -> Self {
        let sender = self.sender;
        self.move_call(
            package_id,
            "object_basics",
            "create_party",
            vec![
                CallArg::Pure(bcs::to_bytes(&1000u64).unwrap()),
                CallArg::Pure(bcs::to_bytes(&sender).unwrap()),
            ],
        )
    }

    pub fn call_object_party_transfer_single_owner(
        self,
        package_id: ObjectID,
        object_arg: ObjectArg,
        recipient: SuiAddress,
    ) -> Self {
        self.move_call(
            package_id,
            "object_basics",
            "party_transfer_single_owner",
            vec![
                CallArg::Object(object_arg),
                CallArg::Pure(bcs::to_bytes(&recipient).unwrap()),
            ],
        )
    }

    pub fn call_object_delete(self, package_id: ObjectID, object_arg: ObjectArg) -> Self {
        self.move_call(
            package_id,
            "object_basics",
            "delete",
            vec![CallArg::Object(object_arg)],
        )
    }

    pub fn transfer(mut self, object: FullObjectRef, recipient: SuiAddress) -> Self {
        self.test_data = TestTransactionData::Transfer(TransferData { object, recipient });
        self
    }

    pub fn transfer_sui(mut self, amount: Option<u64>, recipient: SuiAddress) -> Self {
        self.test_data = TestTransactionData::TransferSui(TransferSuiData { amount, recipient });
        self
    }

    pub fn transfer_sui_to_address_balance(
        mut self,
        source: FundSource,
        amounts_and_recipients: Vec<(u64, SuiAddress)>,
    ) -> Self {
        self.test_data =
            TestTransactionData::TransferFundsToAddressBalance(TransferFundsToAddressBalanceData {
                source,
                amounts_and_recipients,
                type_arg: GAS::type_tag(),
            });
        self
    }

    pub fn transfer_funds_to_address_balance(
        mut self,
        source: FundSource,
        amounts_and_recipients: Vec<(u64, SuiAddress)>,
        type_arg: TypeTag,
    ) -> Self {
        self.test_data =
            TestTransactionData::TransferFundsToAddressBalance(TransferFundsToAddressBalanceData {
                source,
                amounts_and_recipients,
                type_arg,
            });
        self
    }

    pub fn split_coin(mut self, coin: ObjectRef, amounts: Vec<u64>) -> Self {
        self.test_data = TestTransactionData::SplitCoin(SplitCoinData { coin, amounts });
        self
    }

    pub fn publish(mut self, path: PathBuf) -> Self {
        assert!(matches!(self.test_data, TestTransactionData::Empty));
        self.test_data = TestTransactionData::Publish(PublishData::Source(path, false));
        self
    }

    pub fn publish_with_deps(mut self, path: PathBuf) -> Self {
        assert!(matches!(self.test_data, TestTransactionData::Empty));
        self.test_data = TestTransactionData::Publish(PublishData::Source(path, true));
        self
    }

    pub fn publish_with_data(mut self, data: PublishData) -> Self {
        assert!(matches!(self.test_data, TestTransactionData::Empty));
        self.test_data = TestTransactionData::Publish(data);
        self
    }

    pub fn publish_examples(self, subpath: &'static str) -> Self {
        let path = if let Ok(p) = std::env::var("MOVE_EXAMPLES_DIR") {
            let mut path = PathBuf::from(p);
            path.extend([subpath]);
            path
        } else {
            let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            path.extend(["..", "..", "examples", "move", subpath]);
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
                self.gas_budget
                    .unwrap_or(self.gas_price * TEST_ONLY_GAS_UNIT_FOR_HEAVY_COMPUTATION_STORAGE),
                self.gas_price,
            )
            .unwrap(),
            TestTransactionData::Transfer(data) => TransactionData::new_transfer(
                data.recipient,
                data.object,
                self.sender,
                self.gas_object,
                self.gas_budget
                    .unwrap_or(self.gas_price * TEST_ONLY_GAS_UNIT_FOR_TRANSFER),
                self.gas_price,
            ),
            TestTransactionData::TransferSui(data) => TransactionData::new_transfer_sui(
                data.recipient,
                self.sender,
                data.amount,
                self.gas_object,
                self.gas_budget
                    .unwrap_or(self.gas_price * TEST_ONLY_GAS_UNIT_FOR_TRANSFER),
                self.gas_price,
            ),
            TestTransactionData::TransferFundsToAddressBalance(data) => {
                let mut builder = ProgrammableTransactionBuilder::new();
                let source = match data.source.clone() {
                    FundSource::Coin(coin) => {
                        if coin == self.gas_object {
                            Argument::GasCoin
                        } else {
                            builder.obj(ObjectArg::ImmOrOwnedObject(coin)).unwrap()
                        }
                    }
                    FundSource::AddressFund { reservation } => {
                        let reservation = reservation.unwrap_or_else(|| {
                            data.amounts_and_recipients
                                .iter()
                                .map(|(amount, _)| *amount)
                                .sum::<u64>()
                        });
                        builder
                            .funds_withdrawal(FundsWithdrawalArg::balance_from_sender(
                                reservation,
                                data.type_arg.clone().into(),
                            ))
                            .unwrap()
                    }
                };
                for (amount, recipient) in data.amounts_and_recipients {
                    let balance = match data.source.clone() {
                        FundSource::Coin(_) => {
                            let amount_arg = builder.pure(amount).unwrap();
                            let coin = builder.programmable_move_call(
                                SUI_FRAMEWORK_PACKAGE_ID,
                                Identifier::new("coin").unwrap(),
                                Identifier::new("split").unwrap(),
                                vec![data.type_arg.clone()],
                                vec![source, amount_arg],
                            );
                            builder.programmable_move_call(
                                SUI_FRAMEWORK_PACKAGE_ID,
                                Identifier::new("coin").unwrap(),
                                Identifier::new("into_balance").unwrap(),
                                vec![data.type_arg.clone()],
                                vec![coin],
                            )
                        }
                        FundSource::AddressFund { .. } => {
                            let amount_arg = builder.pure(U256::from(amount)).unwrap();
                            let split = builder.programmable_move_call(
                                SUI_FRAMEWORK_PACKAGE_ID,
                                Identifier::new("funds_accumulator").unwrap(),
                                Identifier::new("withdrawal_split").unwrap(),
                                vec![Balance::type_tag(data.type_arg.clone())],
                                vec![source, amount_arg],
                            );
                            builder.programmable_move_call(
                                SUI_FRAMEWORK_PACKAGE_ID,
                                Identifier::new("balance").unwrap(),
                                Identifier::new("redeem_funds").unwrap(),
                                vec![data.type_arg.clone()],
                                vec![split],
                            )
                        }
                    };

                    let recipient_arg = builder.pure(recipient).unwrap();
                    builder.programmable_move_call(
                        SUI_FRAMEWORK_PACKAGE_ID,
                        Identifier::new("balance").unwrap(),
                        Identifier::new("send_funds").unwrap(),
                        vec![data.type_arg.clone()],
                        vec![balance, recipient_arg],
                    );
                }
                let pt = builder.finish();
                TransactionData::new_programmable(
                    self.sender,
                    vec![self.gas_object],
                    pt,
                    self.gas_budget.unwrap_or(
                        self.gas_price * TEST_ONLY_GAS_UNIT_FOR_HEAVY_COMPUTATION_STORAGE,
                    ),
                    self.gas_price,
                )
            }
            TestTransactionData::SplitCoin(data) => TransactionData::new_split_coin(
                self.sender,
                data.coin,
                data.amounts,
                self.gas_object,
                self.gas_budget
                    .unwrap_or(self.gas_price * TEST_ONLY_GAS_UNIT_FOR_TRANSFER),
                self.gas_price,
            ),
            TestTransactionData::Publish(data) => {
                let (all_module_bytes, dependencies) = match data {
                    PublishData::Source(path, with_unpublished_deps) => {
                        let compiled_package = BuildConfig::new_for_testing().build(&path).unwrap();
                        let all_module_bytes =
                            compiled_package.get_package_bytes(with_unpublished_deps);
                        let dependencies = compiled_package.get_dependency_storage_package_ids();
                        (all_module_bytes, dependencies)
                    }
                    PublishData::ModuleBytes(bytecode) => (bytecode, vec![]),
                    PublishData::CompiledPackage(compiled_package) => {
                        let all_module_bytes = compiled_package.get_package_bytes(false);
                        let dependencies = compiled_package.get_dependency_storage_package_ids();
                        (all_module_bytes, dependencies)
                    }
                };

                TransactionData::new_module(
                    self.sender,
                    self.gas_object,
                    all_module_bytes,
                    dependencies,
                    self.gas_budget.unwrap_or(
                        self.gas_price * TEST_ONLY_GAS_UNIT_FOR_HEAVY_COMPUTATION_STORAGE,
                    ),
                    self.gas_price,
                )
            }
            TestTransactionData::Programmable(pt) => TransactionData::new_programmable(
                self.sender,
                vec![self.gas_object],
                pt,
                self.gas_budget
                    .unwrap_or(self.gas_price * TEST_ONLY_GAS_UNIT_FOR_HEAVY_COMPUTATION_STORAGE),
                self.gas_price,
            ),
            TestTransactionData::Empty => {
                panic!("Cannot build empty transaction");
            }
        }
    }

    pub fn build_and_sign(self, signer: &dyn Signer<Signature>) -> Transaction {
        Transaction::from_data_and_signer(self.build(), vec![signer])
    }

    pub fn build_and_sign_multisig(
        self,
        multisig_pk: MultiSigPublicKey,
        signers: &[&dyn Signer<Signature>],
        bitmap: BitmapUnit,
    ) -> Transaction {
        let data = self.build();
        let intent_msg = IntentMessage::new(Intent::sui_transaction(), data.clone());

        let mut signatures = Vec::with_capacity(signers.len());
        for signer in signers {
            signatures.push(
                GenericSignature::from(Signature::new_secure(&intent_msg, *signer))
                    .to_compressed()
                    .unwrap(),
            );
        }

        let multisig =
            GenericSignature::MultiSig(MultiSig::insecure_new(signatures, bitmap, multisig_pk));

        Transaction::from_generic_sig_data(data, vec![multisig])
    }

    pub fn build_and_sign_multisig_legacy(
        self,
        multisig_pk: MultiSigPublicKeyLegacy,
        signers: &[&dyn Signer<Signature>],
    ) -> Transaction {
        let data = self.build();
        let intent = Intent::sui_transaction();
        let intent_msg = IntentMessage::new(intent.clone(), data.clone());

        let mut signatures = Vec::with_capacity(signers.len());
        for signer in signers {
            signatures.push(Signature::new_secure(&intent_msg, *signer).into());
        }

        let multisig = GenericSignature::MultiSigLegacy(
            MultiSigLegacy::combine(signatures, multisig_pk).unwrap(),
        );

        Transaction::from_generic_sig_data(data, vec![multisig])
    }
}

#[allow(clippy::large_enum_variant)]
enum TestTransactionData {
    Move(MoveData),
    Transfer(TransferData),
    TransferSui(TransferSuiData),
    TransferFundsToAddressBalance(TransferFundsToAddressBalanceData),
    SplitCoin(SplitCoinData),
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

#[allow(clippy::large_enum_variant)]
pub enum PublishData {
    /// Path to source code directory and with_unpublished_deps.
    /// with_unpublished_deps indicates whether to publish unpublished dependencies in the same transaction or not.
    Source(PathBuf, bool),
    ModuleBytes(Vec<Vec<u8>>),
    CompiledPackage(CompiledPackage),
}

struct TransferData {
    object: FullObjectRef,
    recipient: SuiAddress,
}

struct TransferSuiData {
    amount: Option<u64>,
    recipient: SuiAddress,
}

struct TransferFundsToAddressBalanceData {
    source: FundSource,
    amounts_and_recipients: Vec<(u64, SuiAddress)>,
    type_arg: TypeTag,
}

struct SplitCoinData {
    coin: ObjectRef,
    amounts: Vec<u64>,
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
    let result = context.get_all_accounts_and_gas_objects().await;
    let accounts_and_objs = result.unwrap();
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
            let tx = context.sign_transaction(&data).await;
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
    context
        .sign_transaction(
            &TestTransactionBuilder::new(sender, gas_object, gas_price)
                .transfer_sui(amount, recipient.unwrap_or(sender))
                .build(),
        )
        .await
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
    context
        .sign_transaction(
            &TestTransactionBuilder::new(sender, gas_object, gas_price)
                .call_staking(stake_object, validator_address)
                .build(),
        )
        .await
}

pub async fn make_publish_transaction(context: &WalletContext, path: PathBuf) -> Transaction {
    let (sender, gas_object) = context.get_one_gas_object().await.unwrap().unwrap();
    let gas_price = context.get_reference_gas_price().await.unwrap();
    context
        .sign_transaction(
            &TestTransactionBuilder::new(sender, gas_object, gas_price)
                .publish(path)
                .build(),
        )
        .await
}

pub async fn make_publish_transaction_with_deps(
    context: &WalletContext,
    path: PathBuf,
) -> Transaction {
    let (sender, gas_object) = context.get_one_gas_object().await.unwrap().unwrap();
    let gas_price = context.get_reference_gas_price().await.unwrap();
    context
        .sign_transaction(
            &TestTransactionBuilder::new(sender, gas_object, gas_price)
                .publish_with_deps(path)
                .build(),
        )
        .await
}

pub async fn publish_package(context: &WalletContext, path: PathBuf) -> ObjectRef {
    let (sender, gas_object) = context.get_one_gas_object().await.unwrap().unwrap();
    let gas_price = context.get_reference_gas_price().await.unwrap();
    let txn = context
        .sign_transaction(
            &TestTransactionBuilder::new(sender, gas_object, gas_price)
                .publish(path)
                .build(),
        )
        .await;
    let resp = context.execute_transaction_must_succeed(txn).await;
    resp.get_new_package_obj().unwrap()
}

/// Executes a transaction to publish the `basics` package and returns the package object ref.
pub async fn publish_basics_package(context: &WalletContext) -> ObjectRef {
    let (sender, gas_object) = context.get_one_gas_object().await.unwrap().unwrap();
    let gas_price = context.get_reference_gas_price().await.unwrap();
    let txn = context
        .sign_transaction(
            &TestTransactionBuilder::new(sender, gas_object, gas_price)
                .publish_examples("basics")
                .build(),
        )
        .await;
    let resp = context.execute_transaction_must_succeed(txn).await;
    resp.get_new_package_obj().unwrap()
}

/// Executes a transaction to publish the `basics` package and another one to create a counter.
/// Returns the package object ref and the counter object ref.
pub async fn publish_basics_package_and_make_counter(
    context: &WalletContext,
) -> (ObjectRef, ObjectRef) {
    let package_ref = publish_basics_package(context).await;
    let (sender, gas_object) = context.get_one_gas_object().await.unwrap().unwrap();
    let gas_price = context.get_reference_gas_price().await.unwrap();
    let counter_creation_txn = context
        .sign_transaction(
            &TestTransactionBuilder::new(sender, gas_object, gas_price)
                .call_counter_create(package_ref.0)
                .build(),
        )
        .await;
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

/// Executes a transaction to publish the `basics` package and another one to create a party
/// object. Returns the package object ref and the counter object ref.
pub async fn publish_basics_package_and_make_party_object(
    context: &WalletContext,
) -> (ObjectRef, ObjectRef) {
    let package_ref = publish_basics_package(context).await;
    let (sender, gas_object) = context.get_one_gas_object().await.unwrap().unwrap();
    let gas_price = context.get_reference_gas_price().await.unwrap();
    let object_creation_txn = context
        .sign_transaction(
            &TestTransactionBuilder::new(sender, gas_object, gas_price)
                .call_object_create_party(package_ref.0)
                .build(),
        )
        .await;
    let resp = context
        .execute_transaction_must_succeed(object_creation_txn)
        .await;
    let object_ref = resp
        .effects
        .unwrap()
        .created()
        .iter()
        .find(|obj_ref| matches!(obj_ref.owner, Owner::ConsensusAddressOwner { .. }))
        .unwrap()
        .reference
        .to_object_ref();
    (package_ref, object_ref)
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
    let txn = context
        .sign_transaction(
            &TestTransactionBuilder::new(sender, gas_object, rgp)
                .call_counter_increment(package_id, counter_id, initial_shared_version)
                .build(),
        )
        .await;
    context.execute_transaction_must_succeed(txn).await
}

/// Executes a transaction that generates a new random u128 using Random and emits it as an event.
pub async fn emit_new_random_u128(
    context: &WalletContext,
    package_id: ObjectID,
) -> SuiTransactionBlockResponse {
    let (sender, gas_object) = context.get_one_gas_object().await.unwrap().unwrap();
    let rgp = context.get_reference_gas_price().await.unwrap();

    let client = context.get_client().await.unwrap();
    let random_obj = client
        .read_api()
        .get_object_with_options(
            SUI_RANDOMNESS_STATE_OBJECT_ID,
            SuiObjectDataOptions::new().with_owner(),
        )
        .await
        .unwrap()
        .into_object()
        .unwrap();
    let random_obj_owner = random_obj
        .owner
        .expect("Expect Randomness object to have an owner");

    let Owner::Shared {
        initial_shared_version,
    } = random_obj_owner
    else {
        panic!("Expect Randomness to be shared object")
    };
    let random_call_arg = CallArg::Object(ObjectArg::SharedObject {
        id: SUI_RANDOMNESS_STATE_OBJECT_ID,
        initial_shared_version,
        mutability: SharedObjectMutability::Immutable,
    });

    let txn = context
        .sign_transaction(
            &TestTransactionBuilder::new(sender, gas_object, rgp)
                .move_call(package_id, "random", "new", vec![random_call_arg])
                .build(),
        )
        .await;
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
    let txn = context
        .sign_transaction(
            &TestTransactionBuilder::new(sender, gas_object, gas_price)
                .publish_examples("nft")
                .build(),
        )
        .await;
    let resp = context.execute_transaction_must_succeed(txn).await;
    let package_id = resp.get_new_package_obj().unwrap().0;
    (package_id, gas_id, resp.digest)
}

/// Pre-requisite: `publish_nfts_package` must be called before this function.  Executes a
/// transaction to create an NFT and returns the sender address, the object id of the NFT, and the
/// digest of the transaction.
pub async fn create_nft(
    context: &WalletContext,
    package_id: ObjectID,
) -> (SuiAddress, ObjectID, TransactionDigest) {
    let (sender, gas_object) = context.get_one_gas_object().await.unwrap().unwrap();
    let rgp = context.get_reference_gas_price().await.unwrap();

    let txn = context
        .sign_transaction(
            &TestTransactionBuilder::new(sender, gas_object, rgp)
                .call_nft_create(package_id)
                .build(),
        )
        .await;
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
pub async fn delete_nft(
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
    let txn = context
        .sign_transaction(
            &TestTransactionBuilder::new(sender, gas, rgp)
                .call_nft_delete(package_id, nft_to_delete)
                .build(),
        )
        .await;
    context.execute_transaction_must_succeed(txn).await
}
