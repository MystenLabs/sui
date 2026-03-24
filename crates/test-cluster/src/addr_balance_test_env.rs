// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use crate::{TestCluster, TestClusterBuilder};
use move_core_types::identifier::Identifier;
use sui_keys::keystore::AccountKeystore;
use sui_protocol_config::{OverrideGuard, ProtocolConfig, ProtocolVersion};
use sui_test_transaction_builder::{FundSource, TestTransactionBuilder};
use sui_types::{
    SUI_FRAMEWORK_PACKAGE_ID, TypeTag,
    accumulator_metadata::get_accumulator_object_count,
    accumulator_root::{AccumulatorValue, U128},
    balance::Balance,
    base_types::{FullObjectRef, ObjectID, ObjectRef, SequenceNumber, SuiAddress},
    coin_reservation::ParsedObjectRefWithdrawal,
    digests::{ChainIdentifier, TransactionDigest},
    effects::{TransactionEffects, TransactionEffectsAPI},
    error::SuiResult,
    gas_coin::GAS,
    object::Owner,
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    storage::ChildObjectResolver,
    transaction::{
        CallArg, FundsWithdrawalArg, GasData, ObjectArg, TransactionData, TransactionDataV1,
        TransactionExpiration, TransactionKind,
    },
};

// TODO: Some of this code may be useful for tests other than address balance tests,
// we might want to rename it and expand its usage.

pub struct TestEnvBuilder {
    num_validators: usize,
    test_cluster_builder_cb: Option<Box<dyn Fn(TestClusterBuilder) -> TestClusterBuilder + Send>>,
    proto_override_cb:
        Option<Box<dyn Fn(ProtocolVersion, ProtocolConfig) -> ProtocolConfig + Send>>,
}

impl Default for TestEnvBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl TestEnvBuilder {
    pub fn new() -> Self {
        Self {
            test_cluster_builder_cb: None,
            proto_override_cb: None,
            num_validators: 1,
        }
    }

    #[allow(dead_code)]
    pub fn with_num_validators(mut self, num_validators: usize) -> Self {
        self.num_validators = num_validators;
        self
    }

    pub fn with_proto_override_cb(
        mut self,
        cb: Box<dyn Fn(ProtocolVersion, ProtocolConfig) -> ProtocolConfig + Send>,
    ) -> Self {
        self.proto_override_cb = Some(cb);
        self
    }

    pub fn with_test_cluster_builder_cb(
        mut self,
        cb: Box<dyn Fn(TestClusterBuilder) -> TestClusterBuilder + Send>,
    ) -> Self {
        self.test_cluster_builder_cb = Some(cb);
        self
    }

    pub async fn build(self) -> TestEnv {
        let _guard = self
            .proto_override_cb
            .map(ProtocolConfig::apply_overrides_for_testing);

        let mut test_cluster_builder =
            TestClusterBuilder::new().with_num_validators(self.num_validators);

        if let Some(cb) = self.test_cluster_builder_cb {
            test_cluster_builder = cb(test_cluster_builder);
        }

        let test_cluster = test_cluster_builder.build().await;

        let chain_id = test_cluster.get_chain_identifier();
        let rgp = test_cluster.get_reference_gas_price().await;

        let mut test_env = TestEnv {
            cluster: test_cluster,
            _guard,
            rgp,
            chain_id,
            gas_objects: BTreeMap::new(),
        };

        test_env.update_all_gas().await;
        test_env
    }
}

pub struct TestEnv {
    pub cluster: TestCluster,
    _guard: Option<OverrideGuard>,
    pub rgp: u64,
    pub chain_id: ChainIdentifier,
    pub gas_objects: BTreeMap<SuiAddress, Vec<ObjectRef>>,
}

impl TestEnv {
    pub async fn update_all_gas(&mut self) {
        // load all gas objects
        let addresses = self.cluster.wallet.config.keystore.addresses();
        self.gas_objects.clear();

        for address in addresses {
            let gas: Vec<_> = self
                .cluster
                .wallet
                .gas_objects(address)
                .await
                .unwrap()
                .into_iter()
                .map(|(_, obj)| obj.compute_object_reference())
                .collect();

            self.gas_objects.insert(address, gas);
        }
    }

    pub async fn fund_one_address_balance(&mut self, address: SuiAddress, amount: u64) {
        let gas = self.gas_objects[&address][0];
        let tx = TestTransactionBuilder::new(address, gas, self.rgp)
            .transfer_sui_to_address_balance(FundSource::coin(gas), vec![(amount, address)])
            .build();
        let (digest, effects) = self
            .cluster
            .sign_and_execute_transaction_directly(&tx)
            .await
            .unwrap();
        // update the gas object we used.
        self.gas_objects.get_mut(&address).unwrap()[0] = effects.gas_object().unwrap().0;
        self.cluster.wait_for_tx_settlement(&[digest]).await;
    }

    #[allow(dead_code)]
    pub async fn fund_all_address_balances(&mut self, amount: u64) {
        let senders = self.gas_objects.keys().copied().collect::<Vec<_>>();
        for sender in senders {
            self.fund_one_address_balance(sender, amount).await;
        }
    }

    pub fn get_sender(&self, index: usize) -> SuiAddress {
        self.gas_objects.keys().copied().nth(index).unwrap()
    }

    pub fn get_sender_and_gas(&self, index: usize) -> (SuiAddress, ObjectRef) {
        let sender = self.get_sender(index);
        let gas = self.gas_objects[&sender][0];
        (sender, gas)
    }

    pub fn get_sender_and_all_gas(&self, index: usize) -> (SuiAddress, Vec<ObjectRef>) {
        let sender = self.get_sender(index);
        let gas = self.gas_objects[&sender].clone();
        (sender, gas)
    }

    pub fn get_all_senders(&self) -> Vec<SuiAddress> {
        self.cluster.wallet.get_addresses()
    }

    pub fn get_gas_for_sender(&self, sender: SuiAddress) -> Vec<ObjectRef> {
        self.gas_objects.get(&sender).unwrap().clone()
    }

    pub fn tx_builder(&self, sender: SuiAddress) -> TestTransactionBuilder {
        let gas = self.gas_objects.get(&sender).unwrap()[0];
        TestTransactionBuilder::new(sender, gas, self.rgp)
    }

    pub fn tx_builder_with_gas(
        &self,
        sender: SuiAddress,
        gas: ObjectRef,
    ) -> TestTransactionBuilder {
        TestTransactionBuilder::new(sender, gas, self.rgp)
    }

    pub fn tx_builder_with_gas_objects(
        &self,
        sender: SuiAddress,
        gas_objects: Vec<ObjectRef>,
    ) -> TestTransactionBuilder {
        TestTransactionBuilder::new_with_gas_objects(sender, gas_objects, self.rgp)
    }

    pub async fn exec_tx_directly(
        &mut self,
        tx: TransactionData,
    ) -> SuiResult<(TransactionDigest, TransactionEffects)> {
        let res = self
            .cluster
            .sign_and_execute_transaction_directly(&tx)
            .await;
        self.update_all_gas().await;
        res
    }

    pub async fn setup_test_package(&mut self, path: impl AsRef<Path>) -> ObjectID {
        let context = &mut self.cluster.wallet;
        let (sender, gas_object) = context.get_one_gas_object().await.unwrap().unwrap();
        let gas_price = context.get_reference_gas_price().await.unwrap();
        let txn = context
            .sign_transaction(
                &TestTransactionBuilder::new(sender, gas_object, gas_price)
                    .publish_async(path.as_ref().to_path_buf())
                    .await
                    .build(),
            )
            .await;
        let resp = context.execute_transaction_must_succeed(txn).await;
        let package_ref = resp.get_new_package_obj().unwrap();
        self.update_all_gas().await;
        package_ref.0
    }

    pub async fn setup_custom_coin(&mut self) -> (SuiAddress, TypeTag) {
        let (publisher, package_id, _) = self.publish_coins_package().await;
        let coin_a_type: TypeTag = format!("{}::coin_a::COIN_A", package_id).parse().unwrap();
        (publisher, coin_a_type)
    }

    /// Publish the coins package and return (publisher, package_id, coin_type, treasury_cap_ref).
    /// The MINTABLE_COIN TreasuryCap is unfrozen so new Coin objects can be minted.
    pub async fn setup_mintable_coin(&mut self) -> (SuiAddress, ObjectID, TypeTag, ObjectRef) {
        let (publisher, package_id, effects) = self.publish_coins_package().await;
        let coin_type: TypeTag = format!("{}::mintable_coin::MINTABLE_COIN", package_id)
            .parse()
            .unwrap();
        let treasury_cap_ref = self.find_created_treasury_cap(&effects).await;
        (publisher, package_id, coin_type, treasury_cap_ref)
    }

    /// Publish the coins test package and return (publisher, package_id, effects).
    async fn publish_coins_package(&mut self) -> (SuiAddress, ObjectID, TransactionEffects) {
        let mut path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.extend(["..", "sui-e2e-tests", "tests", "data", "coins"]);
        let (publisher, gas) = self
            .cluster
            .wallet
            .get_one_gas_object()
            .await
            .unwrap()
            .unwrap();
        let tx = TestTransactionBuilder::new(publisher, gas, self.rgp)
            .publish_async(path)
            .await
            .build();
        let (_, effects) = self.exec_tx_directly(tx).await.unwrap();
        assert!(
            effects.status().is_ok(),
            "Publish failed: {:?}",
            effects.status()
        );
        let package_id = self.find_created_package(&effects).await;
        (publisher, package_id, effects)
    }

    async fn find_created_package(&self, effects: &TransactionEffects) -> ObjectID {
        for (obj_ref, owner) in effects.created() {
            if owner.is_immutable()
                && let Some(obj) = self
                    .cluster
                    .get_object_from_fullnode_store(&obj_ref.0)
                    .await
                && obj.is_package()
            {
                return obj_ref.0;
            }
        }
        panic!("Package should exist among created objects");
    }

    async fn find_created_treasury_cap(&self, effects: &TransactionEffects) -> ObjectRef {
        for (obj_ref, owner) in effects.created() {
            if matches!(owner, Owner::AddressOwner(_))
                && let Some(obj) = self
                    .cluster
                    .get_object_from_fullnode_store(&obj_ref.0)
                    .await
                && obj
                    .data
                    .try_as_move()
                    .is_some_and(|m| m.type_().is_treasury_cap())
            {
                return obj_ref;
            }
        }
        panic!("TreasuryCap should exist among created objects");
    }

    /// Mint a `Coin<T>` of the given amount to the recipient using the `TreasuryCap`.
    /// Returns the updated `TreasuryCap` ref and the new `Coin` object ref.
    pub async fn mint_coin(
        &mut self,
        publisher: SuiAddress,
        package_id: ObjectID,
        treasury_cap_ref: ObjectRef,
        amount: u64,
        recipient: SuiAddress,
    ) -> (ObjectRef, ObjectRef) {
        let tx = self
            .tx_builder(publisher)
            .move_call(
                package_id,
                "mintable_coin",
                "mint_and_transfer",
                vec![
                    CallArg::Object(ObjectArg::ImmOrOwnedObject(treasury_cap_ref)),
                    CallArg::Pure(bcs::to_bytes(&amount).unwrap()),
                    CallArg::Pure(bcs::to_bytes(&recipient).unwrap()),
                ],
            )
            .build();
        let (_, effects) = self.exec_tx_directly(tx).await.unwrap();
        assert!(
            effects.status().is_ok(),
            "Mint failed: {:?}",
            effects.status()
        );
        let new_treasury_cap_ref = effects
            .mutated()
            .into_iter()
            .find(|(obj_ref, _)| obj_ref.0 == treasury_cap_ref.0)
            .unwrap()
            .0;
        let coin_ref = effects
            .created()
            .into_iter()
            .find(|(_, owner)| matches!(owner, Owner::AddressOwner(addr) if *addr == recipient))
            .unwrap()
            .0;
        (new_treasury_cap_ref, coin_ref)
    }

    pub fn encode_coin_reservation(
        &self,
        sender: SuiAddress,
        epoch: u64,
        amount: u64,
    ) -> ObjectRef {
        let accumulator_obj_id = get_sui_accumulator_object_id(sender);
        ParsedObjectRefWithdrawal::new(accumulator_obj_id, epoch, amount)
            .encode(SequenceNumber::new(), self.chain_id)
    }

    pub fn encode_coin_reservation_for_type(
        &self,
        sender: SuiAddress,
        epoch: u64,
        amount: u64,
        coin_type: TypeTag,
    ) -> ObjectRef {
        let accumulator_obj_id = get_accumulator_object_id(sender, coin_type);
        ParsedObjectRefWithdrawal::new(accumulator_obj_id, epoch, amount)
            .encode(SequenceNumber::new(), self.chain_id)
    }

    /// Transfer a portion of a coin to one or more addresses.
    pub async fn transfer_from_coin_to_address_balance(
        &mut self,
        sender: SuiAddress,
        coin: ObjectRef,
        amounts_and_recipients: Vec<(u64, SuiAddress)>,
    ) -> SuiResult<(TransactionDigest, TransactionEffects)> {
        let tx = self
            .tx_builder(sender)
            .transfer_sui_to_address_balance(FundSource::coin(coin), amounts_and_recipients)
            .build();
        let res = self.exec_tx_directly(tx).await;
        self.update_all_gas().await;
        res
    }

    /// Transfer the entire coin to a single address.
    pub async fn transfer_coin_to_address_balance(
        &mut self,
        sender: SuiAddress,
        coin: ObjectRef,
        recipient: SuiAddress,
    ) -> SuiResult<(TransactionDigest, TransactionEffects)> {
        let tx = self
            .tx_builder(sender)
            .transfer(FullObjectRef::from_fastpath_ref(coin), recipient)
            .build();
        let res = self.exec_tx_directly(tx).await;
        self.update_all_gas().await;
        res
    }

    pub fn verify_accumulator_exists(&self, owner: SuiAddress, expected_balance: u64) {
        self.cluster.fullnode_handle.sui_node.with(|node| {
            let state = node.state();
            let child_object_resolver = state.get_child_object_resolver().as_ref();
            verify_accumulator_exists(child_object_resolver, owner, expected_balance);
        });
    }

    /// Verify the accumulator object count after settlement.
    pub fn verify_accumulator_object_count(&self, expected_object_count: u64) {
        self.cluster.fullnode_handle.sui_node.with(|node| {
            let state = node.state();

            let object_count = get_accumulator_object_count(state.get_object_store().as_ref())
                .expect("read cannot fail")
                .expect("accumulator object count should exist after settlement");
            assert_eq!(object_count, expected_object_count);
        });
    }

    /// Get the balance of the owner's SUI address balance.
    pub fn get_sui_balance_ab(&self, owner: SuiAddress) -> u64 {
        self.get_balance_ab(owner, GAS::type_tag())
    }

    pub async fn get_coin_balance(&self, object_id: ObjectID) -> u64 {
        self.cluster
            .get_object_from_fullnode_store(&object_id)
            .await
            .expect("coin object should exist")
            .data
            .try_as_move()
            .expect("should be a Move object")
            .get_coin_value_unsafe()
    }

    /// Get the balance of the owner's address balance for a given coin type.
    pub fn get_balance_ab(&self, owner: SuiAddress, coin_type: TypeTag) -> u64 {
        let db_balance = self.cluster.fullnode_handle.sui_node.with({
            let coin_type = coin_type.clone();
            move |node| {
                let state = node.state();
                let child_object_resolver = state.get_child_object_resolver().as_ref();
                get_balance(child_object_resolver, owner, coin_type)
            }
        });

        let client = self.cluster.grpc_client();
        // Check that the rpc balance agrees with the db balance, on a best-effort basis.
        tokio::task::spawn(async move {
            match client
                .get_balance(owner, &coin_type.to_canonical_string(true).parse().unwrap())
                .await
            {
                Ok(rpc_balance) => {
                    assert_eq!(db_balance, rpc_balance.address_balance());
                }
                Err(e) => {
                    // this usually just means the cluster shut down first before the rpc
                    // completed.
                    tracing::info!("Failed to verify balance via gRPC: {e}");
                }
            }
        });

        db_balance
    }

    /// Get the total balance of SUI owned by the address (including address balance and coins).
    pub async fn get_sui_balance(&self, owner: SuiAddress) -> u64 {
        self.get_balance_for_coin_type(owner, GAS::type_tag()).await
    }

    /// Get the total balance of a given coin type owned by the address (including address balance and coins).
    pub async fn get_balance_for_coin_type(&self, owner: SuiAddress, coin_type: TypeTag) -> u64 {
        let client = self.cluster.grpc_client();
        let rpc_balance = client
            .get_balance(owner, &coin_type.to_canonical_string(true).parse().unwrap())
            .await
            .unwrap();
        rpc_balance.balance()
    }

    pub fn verify_accumulator_removed(&self, owner: SuiAddress) {
        self.cluster.fullnode_handle.sui_node.with(|node| {
            let state = node.state();
            let child_object_resolver = state.get_child_object_resolver().as_ref();
            let sui_coin_type = Balance::type_tag(GAS::type_tag());
            assert!(
                !AccumulatorValue::exists(child_object_resolver, None, owner, &sui_coin_type)
                    .unwrap(),
                "Accumulator value should have been removed"
            );
        });
    }

    pub async fn trigger_reconfiguration(&self) {
        self.cluster.trigger_reconfiguration().await;
    }

    pub fn create_gasless_transaction(
        &self,
        amount: u64,
        token_type: TypeTag,
        sender: SuiAddress,
        recipient: SuiAddress,
        nonce: u32,
        epoch: u64,
    ) -> TransactionData {
        let mut builder = ProgrammableTransactionBuilder::new();
        let withdraw_arg = FundsWithdrawalArg::balance_from_sender(amount, token_type.clone());
        let withdraw_arg = builder.funds_withdrawal(withdraw_arg).unwrap();
        let balance = builder.programmable_move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            Identifier::new("balance").unwrap(),
            Identifier::new("redeem_funds").unwrap(),
            vec![token_type.clone()],
            vec![withdraw_arg],
        );
        let recipient_arg = builder.pure(recipient).unwrap();
        builder.programmable_move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            Identifier::new("balance").unwrap(),
            Identifier::new("send_funds").unwrap(),
            vec![token_type],
            vec![balance, recipient_arg],
        );
        let tx_kind = TransactionKind::ProgrammableTransaction(builder.finish());
        self.gasless_transaction_data(tx_kind, sender, nonce, epoch)
    }

    pub fn gasless_transaction_data(
        &self,
        tx_kind: TransactionKind,
        sender: SuiAddress,
        nonce: u32,
        epoch: u64,
    ) -> TransactionData {
        TransactionData::V1(TransactionDataV1 {
            kind: tx_kind,
            sender,
            gas_data: GasData {
                payment: vec![],
                owner: sender,
                price: 0,
                budget: 0,
            },
            expiration: TransactionExpiration::ValidDuring {
                min_epoch: Some(epoch),
                max_epoch: Some(epoch),
                min_timestamp: None,
                max_timestamp: None,
                chain: self.chain_id,
                nonce,
            },
        })
    }

    /// Publishes the `object_balance` example package, creates an owned vault object,
    /// and funds it with the given amount. Returns (package_id, vault_id).
    pub async fn setup_funded_object_balance_vault(&mut self, amount: u64) -> (ObjectID, ObjectID) {
        let sender = self.get_sender(0);

        let tx = self
            .tx_builder(sender)
            .publish_examples("object_balance")
            .await
            .build();
        let (_, effects) = self.exec_tx_directly(tx).await.unwrap();
        let package_id = effects
            .created()
            .into_iter()
            .find(|(_, owner)| owner.is_immutable())
            .unwrap()
            .0
            .0;

        let tx = self
            .tx_builder(sender)
            .move_call(package_id, "object_balance", "new_owned", vec![])
            .build();
        let (_, effects) = self.exec_tx_directly(tx).await.unwrap();
        let vault_id = effects.created().into_iter().next().unwrap().0.0;

        let tx = self
            .tx_builder(sender)
            .transfer_sui_to_address_balance(
                FundSource::coin(self.get_sender_and_gas(0).1),
                vec![(amount, vault_id.into())],
            )
            .build();
        self.exec_tx_directly(tx).await.unwrap();
        self.trigger_reconfiguration().await;

        (package_id, vault_id)
    }

    /// Publish the trusted_coin package and return (package_id, coin_type, treasury_cap).
    pub async fn publish_trusted_coin(
        &mut self,
        sender: SuiAddress,
    ) -> (ObjectID, TypeTag, ObjectRef) {
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.pop();
        path.extend(["sui-e2e-tests", "tests", "rpc", "data", "trusted_coin"]);

        let tx = self.tx_builder(sender).publish_async(path).await.build();
        let (_, effects) = self.exec_tx_directly(tx).await.unwrap();

        let package_id = effects.published_packages().into_iter().next().unwrap();
        let coin_type: TypeTag = format!("{}::trusted_coin::TRUSTED_COIN", package_id)
            .parse()
            .unwrap();

        // Find the treasury cap by checking object type
        let mut treasury_cap = None;
        for (obj_ref, owner) in effects.created() {
            if owner.is_address_owned() {
                let object = self
                    .cluster
                    .fullnode_handle
                    .sui_node
                    .with_async(
                        |node| async move { node.state().get_object(&obj_ref.0).await.unwrap() },
                    )
                    .await;
                if object.type_().unwrap().name().as_str() == "TreasuryCap" {
                    treasury_cap = Some(obj_ref);
                    break;
                }
            }
        }

        (
            package_id,
            coin_type,
            treasury_cap.expect("Treasury cap not found"),
        )
    }

    /// Mint a trusted coin. Returns (coin_ref, updated_treasury_cap).
    pub async fn mint_trusted_coin(
        &mut self,
        sender: SuiAddress,
        package_id: ObjectID,
        treasury_cap: ObjectRef,
        amount: u64,
    ) -> (ObjectRef, ObjectRef) {
        let tx = self
            .tx_builder(sender)
            .move_call(
                package_id,
                "trusted_coin",
                "mint",
                vec![
                    CallArg::Object(ObjectArg::ImmOrOwnedObject(treasury_cap)),
                    CallArg::Pure(bcs::to_bytes(&amount).unwrap()),
                ],
            )
            .build();
        let (_, effects) = self.exec_tx_directly(tx).await.unwrap();

        let coin_ref = effects
            .created()
            .iter()
            .find(|(_, owner)| owner.is_address_owned())
            .unwrap()
            .0;

        let new_treasury_cap = effects
            .mutated()
            .iter()
            .find(|(obj_ref, _)| obj_ref.0 == treasury_cap.0)
            .map(|(obj_ref, _)| *obj_ref)
            .unwrap();

        (coin_ref, new_treasury_cap)
    }

    /// Transfer a coin to recipient.
    pub async fn transfer_coin(
        &mut self,
        sender: SuiAddress,
        coin: ObjectRef,
        recipient: SuiAddress,
    ) {
        let tx = self
            .tx_builder(sender)
            .transfer(FullObjectRef::from_fastpath_ref(coin), recipient)
            .build();
        self.exec_tx_directly(tx).await.unwrap();
    }

    /// Transfer SUI from sender's gas to recipient.
    pub async fn transfer_sui(&mut self, sender: SuiAddress, recipient: SuiAddress, amount: u64) {
        let tx = self
            .tx_builder(sender)
            .transfer_sui(Some(amount), recipient)
            .build();
        self.exec_tx_directly(tx).await.unwrap();
    }

    /// Transfer SUI from sender's gas to recipient's address balance.
    pub async fn transfer_sui_to_address_balance(
        &mut self,
        sender: SuiAddress,
        recipient: SuiAddress,
        amount: u64,
    ) {
        let gas = self.gas_objects[&sender][0];
        let tx = TestTransactionBuilder::new(sender, gas, self.rgp)
            .transfer_sui_to_address_balance(FundSource::coin(gas), vec![(amount, recipient)])
            .build();
        self.exec_tx_directly(tx).await.unwrap();
    }

    /// Convenience: publish trusted_coin and set up coins for recipient per config.
    pub async fn publish_trusted_coin_and_setup(
        &mut self,
        funder: SuiAddress,
        recipient: SuiAddress,
        config: &CoinTypeConfig,
        coin_amount: u64,
    ) -> (ObjectID, TypeTag) {
        let (package_id, coin_type, mut treasury_cap) = self.publish_trusted_coin(funder).await;

        for _ in 0..config.real_coins {
            let (coin, new_cap) = self
                .mint_trusted_coin(funder, package_id, treasury_cap, coin_amount)
                .await;
            treasury_cap = new_cap;
            self.transfer_coin(funder, coin, recipient).await;
        }

        if config.has_address_balance {
            let (coin, _) = self
                .mint_trusted_coin(funder, package_id, treasury_cap, coin_amount)
                .await;
            let tx = self
                .tx_builder(funder)
                .transfer_funds_to_address_balance(
                    FundSource::Coin(coin),
                    vec![(coin_amount, recipient)],
                    coin_type.clone(),
                )
                .build();
            self.exec_tx_directly(tx).await.unwrap();
        }

        (package_id, coin_type)
    }

    /// Legacy: publish trusted_coin with one real coin and address balance for sender.
    pub async fn publish_and_mint_trusted_coin(
        &mut self,
        sender: SuiAddress,
        amount: u64,
    ) -> (ObjectID, TypeTag) {
        let config = CoinTypeConfig {
            real_coins: 1,
            has_address_balance: true,
        };
        self.publish_trusted_coin_and_setup(sender, sender, &config, amount)
            .await
    }
}

/// Configuration for a single coin type in a test scenario.
#[derive(Clone, Debug)]
pub struct CoinTypeConfig {
    /// Number of real coins to create for this type.
    pub real_coins: usize,
    /// Whether to create an address balance (fake coin) for this type.
    pub has_address_balance: bool,
}

pub fn get_sui_accumulator_object_id(sender: SuiAddress) -> ObjectID {
    get_accumulator_object_id(sender, GAS::type_tag())
}

pub fn get_accumulator_object_id(sender: SuiAddress, coin_type: TypeTag) -> ObjectID {
    *AccumulatorValue::get_field_id(sender, &Balance::type_tag(coin_type))
        .unwrap()
        .inner()
}

pub fn get_balance(
    child_object_resolver: &dyn ChildObjectResolver,
    owner: SuiAddress,
    coin_type: TypeTag,
) -> u64 {
    sui_core::accumulators::balances::get_balance(owner, child_object_resolver, coin_type).unwrap()
}

pub fn get_sui_balance(child_object_resolver: &dyn ChildObjectResolver, owner: SuiAddress) -> u64 {
    get_balance(child_object_resolver, owner, GAS::type_tag())
}

pub fn verify_accumulator_exists(
    child_object_resolver: &dyn ChildObjectResolver,
    owner: SuiAddress,
    expected_balance: u64,
) {
    let sui_coin_type = Balance::type_tag(GAS::type_tag());

    assert!(
        AccumulatorValue::exists(child_object_resolver, None, owner, &sui_coin_type).unwrap(),
        "Accumulator value should have been created"
    );

    let accumulator_object =
        AccumulatorValue::load_object(child_object_resolver, None, owner, &sui_coin_type)
            .expect("read cannot fail")
            .expect("accumulator should exist");

    assert!(
        accumulator_object
            .data
            .try_as_move()
            .unwrap()
            .type_()
            .is_efficient_representation()
    );

    let accumulator_value =
        AccumulatorValue::load(child_object_resolver, None, owner, &sui_coin_type)
            .expect("read cannot fail")
            .expect("accumulator should exist");

    assert_eq!(
        accumulator_value,
        AccumulatorValue::U128(U128 {
            value: expected_balance as u128
        }),
        "Accumulator value should be {expected_balance}"
    );
}
