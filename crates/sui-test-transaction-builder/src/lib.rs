// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::ident_str;
use shared_crypto::intent::Intent;
use std::path::PathBuf;
use sui_move_build::BuildConfig;
use sui_types::base_types::{ObjectID, ObjectRef, SequenceNumber, SuiAddress};
use sui_types::crypto::{Signature, Signer};
use sui_types::sui_system_state::SUI_SYSTEM_MODULE_NAME;
use sui_types::transaction::{
    CallArg, ObjectArg, Transaction, TransactionData, VerifiedTransaction,
    TEST_ONLY_GAS_UNIT_FOR_GENERIC, TEST_ONLY_GAS_UNIT_FOR_PUBLISH,
};
use sui_types::{
    TypeTag, SUI_SYSTEM_OBJECT_ID, SUI_SYSTEM_STATE_OBJECT_ID,
    SUI_SYSTEM_STATE_OBJECT_SHARED_VERSION,
};

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

    pub fn call_staking(self, stake_coin: ObjectRef, validator: SuiAddress) -> Self {
        self.move_call(
            SUI_SYSTEM_OBJECT_ID,
            SUI_SYSTEM_MODULE_NAME.as_str(),
            "request_add_stake",
            vec![
                CallArg::Object(ObjectArg::SharedObject {
                    id: SUI_SYSTEM_STATE_OBJECT_ID,
                    initial_shared_version: SUI_SYSTEM_STATE_OBJECT_SHARED_VERSION,
                    mutable: true,
                }),
                CallArg::Object(ObjectArg::ImmOrOwnedObject(stake_coin)),
                CallArg::Pure(bcs::to_bytes(&validator).unwrap()),
            ],
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

    pub fn publish(mut self, path: PathBuf) -> Self {
        assert!(matches!(self.test_data, TestTransactionData::Empty));
        self.test_data = TestTransactionData::Publish(PublishData { path });
        self
    }

    pub fn publish_examples(self, subpath: &'static str) -> Self {
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.extend(["..", "..", "sui_programmability", "examples", subpath]);
        self.publish(path)
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
                self.gas_price * TEST_ONLY_GAS_UNIT_FOR_GENERIC,
                self.gas_price,
            ),
            TestTransactionData::Publish(data) => {
                let compiled_package = BuildConfig::new_for_testing().build(data.path).unwrap();
                let all_module_bytes =
                    compiled_package.get_package_bytes(/* with_unpublished_deps */ false);
                let dependencies = compiled_package.get_dependency_original_package_ids();

                TransactionData::new_module(
                    self.sender,
                    self.gas_object,
                    all_module_bytes,
                    dependencies,
                    self.gas_price * TEST_ONLY_GAS_UNIT_FOR_PUBLISH,
                    self.gas_price,
                )
            }
            TestTransactionData::Empty => {
                panic!("Cannot build empty transaction");
            }
        }
    }

    pub fn build_and_sign(self, signer: &dyn Signer<Signature>) -> VerifiedTransaction {
        VerifiedTransaction::new_unchecked(Transaction::from_data_and_signer(
            self.build(),
            Intent::sui_transaction(),
            vec![signer],
        ))
    }
}

enum TestTransactionData {
    Move(MoveData),
    Transfer(TransferData),
    Publish(PublishData),
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
}

struct TransferData {
    object: ObjectRef,
    recipient: SuiAddress,
}
