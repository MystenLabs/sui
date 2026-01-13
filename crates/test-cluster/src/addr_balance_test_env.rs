// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use crate::{TestCluster, TestClusterBuilder};
use sui_json_rpc_api::CoinReadApiClient;
use sui_keys::keystore::AccountKeystore;
use sui_protocol_config::{OverrideGuard, ProtocolConfig, ProtocolVersion};
use sui_test_transaction_builder::{FundSource, TestTransactionBuilder};
use sui_types::{
    TypeTag,
    accumulator_metadata::{AccumulatorOwner, get_accumulator_object_count},
    accumulator_root::{AccumulatorValue, U128},
    balance::Balance,
    base_types::{FullObjectRef, ObjectID, ObjectRef, SequenceNumber, SuiAddress},
    coin_reservation::ParsedObjectRefWithdrawal,
    digests::{ChainIdentifier, TransactionDigest},
    effects::{TransactionEffects, TransactionEffectsAPI},
    error::SuiResult,
    gas_coin::GAS,
    storage::ChildObjectResolver,
    transaction::TransactionData,
};

// TODO: Some of this code may be useful for tests other than address balance tests,
// we might want to rename it and expand its usage.

pub struct TestEnvBuilder {
    num_validators: usize,
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

    pub async fn build(self) -> TestEnv {
        let _guard = self
            .proto_override_cb
            .map(ProtocolConfig::apply_overrides_for_testing);

        let test_cluster = TestClusterBuilder::new()
            .with_num_validators(self.num_validators)
            .build()
            .await;

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
                .map(|(_, obj)| obj.object_ref())
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

    pub fn get_sui_balance(&self, owner: SuiAddress) -> u64 {
        self.get_balance(owner, GAS::type_tag())
    }

    pub fn get_balance(&self, owner: SuiAddress, coin_type: TypeTag) -> u64 {
        let db_balance = self.cluster.fullnode_handle.sui_node.with({
            let coin_type = coin_type.clone();
            move |node| {
                let state = node.state();
                let child_object_resolver = state.get_child_object_resolver().as_ref();
                get_balance(child_object_resolver, owner, coin_type)
            }
        });

        let client = self.cluster.fullnode_handle.rpc_client.clone();
        tokio::task::spawn(async move {
            let rpc_balance = client
                .get_balance(owner, Some(coin_type.to_canonical_string(true)))
                .await
                .unwrap();
            assert_eq!(db_balance, rpc_balance.funds_in_address_balance as u64);
        });

        db_balance
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
            assert!(
                !AccumulatorOwner::exists(child_object_resolver, None, owner).unwrap(),
                "Owner object should have been removed"
            );
        });
    }

    pub async fn trigger_reconfiguration(&self) {
        self.cluster.trigger_reconfiguration().await;
    }
}

pub fn get_sui_accumulator_object_id(sender: SuiAddress) -> ObjectID {
    *AccumulatorValue::get_field_id(sender, &Balance::type_tag(GAS::type_tag()))
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

    assert!(
        AccumulatorOwner::exists(child_object_resolver, None, owner).unwrap(),
        "Owner object should have been created"
    );

    let owner_obj = AccumulatorOwner::load(child_object_resolver, None, owner)
        .expect("read cannot fail")
        .expect("owner must exist");

    assert!(
        owner_obj
            .metadata_exists(child_object_resolver, None, &sui_coin_type)
            .unwrap(),
        "Metadata object should have been created"
    );

    let _metadata = owner_obj
        .load_metadata(child_object_resolver, None, &sui_coin_type)
        .unwrap();
}
