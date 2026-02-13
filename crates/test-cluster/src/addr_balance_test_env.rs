// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use crate::{TestCluster, TestClusterBuilder};
use sui_keys::keystore::AccountKeystore;
use sui_protocol_config::{OverrideGuard, ProtocolConfig, ProtocolVersion};
use sui_test_transaction_builder::{FundSource, TestTransactionBuilder};
use sui_types::{
    accumulator_metadata::get_accumulator_object_count,
    accumulator_root::{AccumulatorValue, U128},
    balance::Balance,
    base_types::{FullObjectRef, ObjectID, ObjectRef, SequenceNumber, SuiAddress},
    coin_reservation::ParsedObjectRefWithdrawal,
    digests::{ChainIdentifier, TransactionDigest},
    effects::{TransactionEffects, TransactionEffectsAPI},
    error::SuiResult,
    gas_coin::GAS,
    storage::ChildObjectResolver,
    transaction::{CallArg, ObjectArg, TransactionData},
    TypeTag,
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
        self.gas_objects.get_mut(&address).unwrap()[0] = effects.gas_object().0;
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

    /// Publishes the trusted_coin package and mints coins to the sender's address balance.
    /// Returns the package ID and the coin type tag.
    pub async fn publish_and_mint_trusted_coin(
        &mut self,
        sender: SuiAddress,
        amount: u64,
    ) -> (ObjectID, TypeTag) {
        let test_tx_builder = self.tx_builder(sender);

        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.pop(); // go up from test-cluster to crates
        path.extend(["sui-e2e-tests", "tests", "rpc", "data", "trusted_coin"]);
        let coin_publish = test_tx_builder.publish_async(path).await.build();

        let (_, effects) = self.exec_tx_directly(coin_publish).await.unwrap();

        // Find the treasury cap object
        let treasury_cap = {
            let mut treasury_cap = None;
            for (obj_ref, owner) in effects.created() {
                if owner.is_address_owned() {
                    let object = self
                        .cluster
                        .fullnode_handle
                        .sui_node
                        .with_async(|node| async move {
                            node.state().get_object(&obj_ref.0).await.unwrap()
                        })
                        .await;
                    if object.type_().unwrap().name().as_str() == "TreasuryCap" {
                        treasury_cap = Some(obj_ref);
                        break;
                    }
                }
            }
            treasury_cap.expect("Treasury cap not found")
        };

        // extract the newly published package id.
        let package_id = effects.published_packages().into_iter().next().unwrap();

        // call trusted_coin::mint to mint coins
        let test_tx_builder = self.tx_builder(sender);
        let mint_tx = test_tx_builder
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
        let (_, mint_effects) = self.exec_tx_directly(mint_tx).await.unwrap();

        // the trusted coin is the only address-owned object created.
        let trusted_coin_ref = mint_effects
            .created()
            .iter()
            .find(|(_, owner)| owner.is_address_owned())
            .unwrap()
            .0;

        let coin_type: TypeTag = format!("{}::trusted_coin::TRUSTED_COIN", package_id)
            .parse()
            .unwrap();

        // Transfer the coins to the sender's address balance
        let send_tx = self
            .tx_builder(sender)
            .transfer_funds_to_address_balance(
                FundSource::Coin(trusted_coin_ref),
                vec![(amount, sender)],
                coin_type.clone(),
            )
            .build();
        let (_, send_effects) = self.exec_tx_directly(send_tx).await.unwrap();
        assert!(
            send_effects.status().is_ok(),
            "Transaction should succeed, got: {:?}",
            send_effects.status()
        );

        (package_id, coin_type)
    }
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

    assert!(accumulator_object
        .data
        .try_as_move()
        .unwrap()
        .type_()
        .is_efficient_representation());

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
