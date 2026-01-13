// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use rand::Rng;
use rand::rngs::SmallRng;
use sui_types::TypeTag;
use sui_types::base_types::{ObjectID, SequenceNumber, SuiAddress};
use sui_types::gas_coin::GAS;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::{
    Argument, CallArg, Command, FundsWithdrawalArg, ObjectArg, SharedObjectMutability,
};
use sui_types::{Identifier, SUI_FRAMEWORK_PACKAGE_ID, SUI_RANDOMNESS_STATE_OBJECT_ID};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum InitRequirement {
    SeedAddressBalance,
    CreateBalancePool,
    SeedBalancePool,
    CreateTestCoinCap,
    SeedTestCoinAddressBalance,
}

pub struct OperationDescriptor {
    pub name: &'static str,
    pub flag: u32,
    pub factory: fn() -> Box<dyn Operation>,
}

pub const ALL_OPERATIONS: &[OperationDescriptor] = &[
    SharedCounterIncrement::DESCRIPTOR,
    SharedCounterRead::DESCRIPTOR,
    RandomnessRead::DESCRIPTOR,
    AddressBalanceDeposit::DESCRIPTOR,
    AddressBalanceWithdraw::DESCRIPTOR,
    ObjectBalanceDeposit::DESCRIPTOR,
    ObjectBalanceWithdraw::DESCRIPTOR,
    TestCoinMint::DESCRIPTOR,
    TestCoinAddressDeposit::DESCRIPTOR,
    TestCoinAddressWithdraw::DESCRIPTOR,
    TestCoinObjectWithdraw::DESCRIPTOR,
];

pub fn describe_flags(flags: u32) -> String {
    let names: Vec<&str> = ALL_OPERATIONS
        .iter()
        .filter(|d| (flags & d.flag) != 0)
        .map(|d| d.name)
        .collect();
    if names.is_empty() {
        "empty".to_string()
    } else {
        names.join(" + ")
    }
}

#[derive(Debug, Clone)]
pub enum ResourceRequest {
    SharedCounter,
    Randomness,
    AddressBalance,
    ObjectBalance,
    TestCoinCap,
}

#[derive(Debug, Clone, Default)]
pub struct OperationConstraints {
    pub must_be_last_shared_access: bool,
}

pub struct OperationResources {
    pub counter: Option<(ObjectID, SequenceNumber)>,
    pub randomness: Option<SequenceNumber>,
    pub package_id: ObjectID,
    pub address_balance_amount: u64,
    pub balance_pool: Option<(ObjectID, SequenceNumber)>,
    pub test_coin_cap: Option<(ObjectID, SequenceNumber)>,
    pub test_coin_type: Option<TypeTag>,
}

pub trait Operation: Send + Sync {
    fn name(&self) -> &'static str;
    fn operation_flag(&self) -> u32;
    fn resource_requests(&self) -> Vec<ResourceRequest>;
    fn constraints(&self) -> OperationConstraints {
        OperationConstraints::default()
    }
    fn init_requirements(&self) -> Vec<InitRequirement> {
        vec![]
    }
    fn apply(
        &self,
        builder: &mut ProgrammableTransactionBuilder,
        resources: &OperationResources,
        rng: &mut SmallRng,
    );
}

pub struct SharedCounterIncrement;

impl SharedCounterIncrement {
    pub const FLAG: u32 = 1 << 0;
    pub const DESCRIPTOR: OperationDescriptor = OperationDescriptor {
        name: "SharedCounterIncrement",
        flag: Self::FLAG,
        factory: || Box::new(SharedCounterIncrement),
    };
}

impl Operation for SharedCounterIncrement {
    fn name(&self) -> &'static str {
        "shared_counter_increment"
    }

    fn operation_flag(&self) -> u32 {
        Self::FLAG
    }

    fn resource_requests(&self) -> Vec<ResourceRequest> {
        vec![ResourceRequest::SharedCounter]
    }

    fn apply(
        &self,
        builder: &mut ProgrammableTransactionBuilder,
        resources: &OperationResources,
        _rng: &mut SmallRng,
    ) {
        let (id, initial_shared_version) = resources.counter.expect("Counter not resolved");

        builder
            .move_call(
                resources.package_id,
                Identifier::new("counter").unwrap(),
                Identifier::new("increment").unwrap(),
                vec![],
                vec![CallArg::Object(ObjectArg::SharedObject {
                    id,
                    initial_shared_version,
                    mutability: SharedObjectMutability::Mutable,
                })],
            )
            .unwrap();
    }
}

pub struct SharedCounterRead;

impl SharedCounterRead {
    pub const FLAG: u32 = 1 << 1;
    pub const DESCRIPTOR: OperationDescriptor = OperationDescriptor {
        name: "SharedCounterRead",
        flag: Self::FLAG,
        factory: || Box::new(SharedCounterRead),
    };
}

impl Operation for SharedCounterRead {
    fn name(&self) -> &'static str {
        "shared_counter_read"
    }

    fn operation_flag(&self) -> u32 {
        Self::FLAG
    }

    fn resource_requests(&self) -> Vec<ResourceRequest> {
        vec![ResourceRequest::SharedCounter]
    }

    fn apply(
        &self,
        builder: &mut ProgrammableTransactionBuilder,
        resources: &OperationResources,
        _rng: &mut SmallRng,
    ) {
        let (id, initial_shared_version) = resources.counter.expect("Counter not resolved");

        builder
            .move_call(
                resources.package_id,
                Identifier::new("counter").unwrap(),
                Identifier::new("value").unwrap(),
                vec![],
                vec![CallArg::Object(ObjectArg::SharedObject {
                    id,
                    initial_shared_version,
                    mutability: SharedObjectMutability::Immutable,
                })],
            )
            .unwrap();
    }
}

pub struct RandomnessRead;

impl RandomnessRead {
    pub const FLAG: u32 = 1 << 2;
    pub const DESCRIPTOR: OperationDescriptor = OperationDescriptor {
        name: "RandomnessRead",
        flag: Self::FLAG,
        factory: || Box::new(RandomnessRead),
    };
}

impl Operation for RandomnessRead {
    fn name(&self) -> &'static str {
        "randomness_read"
    }

    fn operation_flag(&self) -> u32 {
        Self::FLAG
    }

    fn resource_requests(&self) -> Vec<ResourceRequest> {
        vec![ResourceRequest::Randomness]
    }

    fn constraints(&self) -> OperationConstraints {
        OperationConstraints {
            must_be_last_shared_access: true,
        }
    }

    fn apply(
        &self,
        builder: &mut ProgrammableTransactionBuilder,
        resources: &OperationResources,
        _rng: &mut SmallRng,
    ) {
        let initial_shared_version = resources.randomness.expect("Randomness not resolved");

        builder
            .move_call(
                resources.package_id,
                Identifier::new("random").unwrap(),
                Identifier::new("new").unwrap(),
                vec![],
                vec![CallArg::Object(ObjectArg::SharedObject {
                    id: SUI_RANDOMNESS_STATE_OBJECT_ID,
                    initial_shared_version,
                    mutability: SharedObjectMutability::Immutable,
                })],
            )
            .unwrap();
    }
}

pub struct AddressBalanceDeposit;

impl AddressBalanceDeposit {
    pub const FLAG: u32 = 1 << 3;
    pub const DESCRIPTOR: OperationDescriptor = OperationDescriptor {
        name: "AddressBalanceDeposit",
        flag: Self::FLAG,
        factory: || Box::new(AddressBalanceDeposit),
    };
}

impl Operation for AddressBalanceDeposit {
    fn name(&self) -> &'static str {
        "address_balance_deposit"
    }

    fn operation_flag(&self) -> u32 {
        Self::FLAG
    }

    fn resource_requests(&self) -> Vec<ResourceRequest> {
        vec![ResourceRequest::AddressBalance]
    }

    fn apply(
        &self,
        builder: &mut ProgrammableTransactionBuilder,
        resources: &OperationResources,
        rng: &mut SmallRng,
    ) {
        let recipient = SuiAddress::random_for_testing_only();

        let amount = if resources.address_balance_amount > 0 {
            resources.address_balance_amount
        } else {
            rng.gen_range(1000..10000)
        };

        let amount_arg = builder.pure(amount).unwrap();
        let coin = builder.command(Command::SplitCoins(Argument::GasCoin, vec![amount_arg]));
        let Argument::Result(coin_idx) = coin else {
            panic!("SplitCoins should return Result");
        };
        let coin = Argument::NestedResult(coin_idx, 0);

        let coin_balance = builder.programmable_move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            Identifier::new("coin").unwrap(),
            Identifier::new("into_balance").unwrap(),
            vec![GAS::type_tag()],
            vec![coin],
        );

        let recipient_arg = builder.pure(recipient).unwrap();
        builder.programmable_move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            Identifier::new("balance").unwrap(),
            Identifier::new("send_funds").unwrap(),
            vec![GAS::type_tag()],
            vec![coin_balance, recipient_arg],
        );
    }
}

pub struct AddressBalanceWithdraw;

impl AddressBalanceWithdraw {
    pub const FLAG: u32 = 1 << 4;
    pub const DESCRIPTOR: OperationDescriptor = OperationDescriptor {
        name: "AddressBalanceWithdraw",
        flag: Self::FLAG,
        factory: || Box::new(AddressBalanceWithdraw),
    };
}

impl Operation for AddressBalanceWithdraw {
    fn name(&self) -> &'static str {
        "address_balance_withdraw"
    }

    fn operation_flag(&self) -> u32 {
        Self::FLAG
    }

    fn resource_requests(&self) -> Vec<ResourceRequest> {
        vec![ResourceRequest::AddressBalance]
    }

    fn init_requirements(&self) -> Vec<InitRequirement> {
        vec![InitRequirement::SeedAddressBalance]
    }

    fn apply(
        &self,
        builder: &mut ProgrammableTransactionBuilder,
        resources: &OperationResources,
        rng: &mut SmallRng,
    ) {
        let amount = if resources.address_balance_amount > 0 {
            resources.address_balance_amount
        } else {
            rng.gen_range(100..1000)
        };

        let withdrawal_arg = FundsWithdrawalArg::balance_from_sender(amount, GAS::type_tag());

        let balance = builder.funds_withdrawal(withdrawal_arg).unwrap();

        let coin = builder.programmable_move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            Identifier::new("coin").unwrap(),
            Identifier::new("redeem_funds").unwrap(),
            vec![GAS::type_tag()],
            vec![balance],
        );

        let recipient = SuiAddress::random_for_testing_only();
        builder.transfer_arg(recipient, coin);
    }
}

pub struct ObjectBalanceDeposit;

impl ObjectBalanceDeposit {
    pub const FLAG: u32 = 1 << 5;
    pub const DESCRIPTOR: OperationDescriptor = OperationDescriptor {
        name: "ObjectBalanceDeposit",
        flag: Self::FLAG,
        factory: || Box::new(ObjectBalanceDeposit),
    };
}

impl Operation for ObjectBalanceDeposit {
    fn name(&self) -> &'static str {
        "object_balance_deposit"
    }

    fn operation_flag(&self) -> u32 {
        Self::FLAG
    }

    fn resource_requests(&self) -> Vec<ResourceRequest> {
        vec![ResourceRequest::ObjectBalance]
    }

    fn init_requirements(&self) -> Vec<InitRequirement> {
        vec![InitRequirement::CreateBalancePool]
    }

    fn apply(
        &self,
        builder: &mut ProgrammableTransactionBuilder,
        resources: &OperationResources,
        rng: &mut SmallRng,
    ) {
        let (pool_id, initial_shared_version) =
            resources.balance_pool.expect("Balance pool not resolved");

        let amount = if resources.address_balance_amount > 0 {
            resources.address_balance_amount
        } else {
            rng.gen_range(1000..10000)
        };

        let amount_arg = builder.pure(amount).unwrap();
        let coin = builder.command(Command::SplitCoins(Argument::GasCoin, vec![amount_arg]));
        let Argument::Result(coin_idx) = coin else {
            panic!("SplitCoins should return Result");
        };
        let coin = Argument::NestedResult(coin_idx, 0);

        let coin_balance = builder.programmable_move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            Identifier::new("coin").unwrap(),
            Identifier::new("into_balance").unwrap(),
            vec![GAS::type_tag()],
            vec![coin],
        );

        let pool_arg = builder
            .obj(ObjectArg::SharedObject {
                id: pool_id,
                initial_shared_version,
                mutability: SharedObjectMutability::Immutable,
            })
            .unwrap();

        builder.programmable_move_call(
            resources.package_id,
            Identifier::new("balance_pool").unwrap(),
            Identifier::new("deposit").unwrap(),
            vec![GAS::type_tag()],
            vec![pool_arg, coin_balance],
        );
    }
}

pub struct ObjectBalanceWithdraw;

impl ObjectBalanceWithdraw {
    pub const FLAG: u32 = 1 << 6;
    pub const DESCRIPTOR: OperationDescriptor = OperationDescriptor {
        name: "ObjectBalanceWithdraw",
        flag: Self::FLAG,
        factory: || Box::new(ObjectBalanceWithdraw),
    };
}

impl Operation for ObjectBalanceWithdraw {
    fn name(&self) -> &'static str {
        "object_balance_withdraw"
    }

    fn operation_flag(&self) -> u32 {
        Self::FLAG
    }

    fn resource_requests(&self) -> Vec<ResourceRequest> {
        vec![ResourceRequest::ObjectBalance]
    }

    fn init_requirements(&self) -> Vec<InitRequirement> {
        vec![
            InitRequirement::CreateBalancePool,
            InitRequirement::SeedBalancePool,
        ]
    }

    fn apply(
        &self,
        builder: &mut ProgrammableTransactionBuilder,
        resources: &OperationResources,
        rng: &mut SmallRng,
    ) {
        let (pool_id, initial_shared_version) =
            resources.balance_pool.expect("Balance pool not resolved");

        let amount = if resources.address_balance_amount > 0 {
            resources.address_balance_amount
        } else {
            rng.gen_range(100..1000)
        };

        let pool_arg = builder
            .obj(ObjectArg::SharedObject {
                id: pool_id,
                initial_shared_version,
                mutability: SharedObjectMutability::Mutable,
            })
            .unwrap();

        let amount_arg = builder.pure(amount).unwrap();

        let withdrawal = builder.programmable_move_call(
            resources.package_id,
            Identifier::new("balance_pool").unwrap(),
            Identifier::new("withdraw").unwrap(),
            vec![GAS::type_tag()],
            vec![pool_arg, amount_arg],
        );

        let balance = builder.programmable_move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            Identifier::new("balance").unwrap(),
            Identifier::new("redeem_funds").unwrap(),
            vec![GAS::type_tag()],
            vec![withdrawal],
        );

        let coin = builder.programmable_move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            Identifier::new("coin").unwrap(),
            Identifier::new("from_balance").unwrap(),
            vec![GAS::type_tag()],
            vec![balance],
        );

        let recipient = SuiAddress::random_for_testing_only();
        builder.transfer_arg(recipient, coin);
    }
}

pub struct TestCoinMint;

impl TestCoinMint {
    pub const FLAG: u32 = 1 << 7;
    pub const DESCRIPTOR: OperationDescriptor = OperationDescriptor {
        name: "TestCoinMint",
        flag: Self::FLAG,
        factory: || Box::new(TestCoinMint),
    };
}

impl Operation for TestCoinMint {
    fn name(&self) -> &'static str {
        "test_coin_mint"
    }

    fn operation_flag(&self) -> u32 {
        Self::FLAG
    }

    fn resource_requests(&self) -> Vec<ResourceRequest> {
        vec![ResourceRequest::TestCoinCap, ResourceRequest::ObjectBalance]
    }

    fn init_requirements(&self) -> Vec<InitRequirement> {
        vec![
            InitRequirement::CreateTestCoinCap,
            InitRequirement::CreateBalancePool,
        ]
    }

    fn apply(
        &self,
        builder: &mut ProgrammableTransactionBuilder,
        resources: &OperationResources,
        rng: &mut SmallRng,
    ) {
        let (cap_id, cap_version) = resources.test_coin_cap.expect("Test coin cap not resolved");
        let (pool_id, pool_version) = resources.balance_pool.expect("Balance pool not resolved");
        let test_coin_type = resources
            .test_coin_type
            .clone()
            .expect("Test coin type not resolved");

        let amount = if resources.address_balance_amount > 0 {
            resources.address_balance_amount
        } else {
            rng.gen_range(1000..10000)
        };

        let cap_arg = builder
            .obj(ObjectArg::SharedObject {
                id: cap_id,
                initial_shared_version: cap_version,
                mutability: SharedObjectMutability::Mutable,
            })
            .unwrap();

        let amount_arg = builder.pure(amount).unwrap();

        let balance = builder.programmable_move_call(
            resources.package_id,
            Identifier::new("test_coin").unwrap(),
            Identifier::new("mint_balance").unwrap(),
            vec![],
            vec![cap_arg, amount_arg],
        );

        let pool_arg = builder
            .obj(ObjectArg::SharedObject {
                id: pool_id,
                initial_shared_version: pool_version,
                mutability: SharedObjectMutability::Immutable,
            })
            .unwrap();

        builder.programmable_move_call(
            resources.package_id,
            Identifier::new("balance_pool").unwrap(),
            Identifier::new("deposit").unwrap(),
            vec![test_coin_type],
            vec![pool_arg, balance],
        );
    }
}

pub struct TestCoinAddressDeposit;

impl TestCoinAddressDeposit {
    pub const FLAG: u32 = 1 << 8;
    pub const DESCRIPTOR: OperationDescriptor = OperationDescriptor {
        name: "TestCoinAddressDeposit",
        flag: Self::FLAG,
        factory: || Box::new(TestCoinAddressDeposit),
    };
}

impl Operation for TestCoinAddressDeposit {
    fn name(&self) -> &'static str {
        "test_coin_address_deposit"
    }

    fn operation_flag(&self) -> u32 {
        Self::FLAG
    }

    fn resource_requests(&self) -> Vec<ResourceRequest> {
        vec![
            ResourceRequest::ObjectBalance,
            ResourceRequest::AddressBalance,
        ]
    }

    fn init_requirements(&self) -> Vec<InitRequirement> {
        vec![InitRequirement::CreateBalancePool]
    }

    fn apply(
        &self,
        builder: &mut ProgrammableTransactionBuilder,
        resources: &OperationResources,
        rng: &mut SmallRng,
    ) {
        let (pool_id, pool_version) = resources.balance_pool.expect("Balance pool not resolved");
        let test_coin_type = resources
            .test_coin_type
            .clone()
            .expect("Test coin type not resolved");
        let recipient = SuiAddress::random_for_testing_only();

        let amount = if resources.address_balance_amount > 0 {
            resources.address_balance_amount
        } else {
            rng.gen_range(100..1000)
        };

        let pool_arg = builder
            .obj(ObjectArg::SharedObject {
                id: pool_id,
                initial_shared_version: pool_version,
                mutability: SharedObjectMutability::Mutable,
            })
            .unwrap();

        let amount_arg = builder.pure(amount).unwrap();

        let withdrawal = builder.programmable_move_call(
            resources.package_id,
            Identifier::new("balance_pool").unwrap(),
            Identifier::new("withdraw").unwrap(),
            vec![test_coin_type.clone()],
            vec![pool_arg, amount_arg],
        );

        let balance = builder.programmable_move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            Identifier::new("balance").unwrap(),
            Identifier::new("redeem_funds").unwrap(),
            vec![test_coin_type.clone()],
            vec![withdrawal],
        );

        let recipient_arg = builder.pure(recipient).unwrap();
        builder.programmable_move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            Identifier::new("balance").unwrap(),
            Identifier::new("send_funds").unwrap(),
            vec![test_coin_type],
            vec![balance, recipient_arg],
        );
    }
}

pub struct TestCoinAddressWithdraw;

impl TestCoinAddressWithdraw {
    pub const FLAG: u32 = 1 << 9;
    pub const DESCRIPTOR: OperationDescriptor = OperationDescriptor {
        name: "TestCoinAddressWithdraw",
        flag: Self::FLAG,
        factory: || Box::new(TestCoinAddressWithdraw),
    };
}

impl Operation for TestCoinAddressWithdraw {
    fn name(&self) -> &'static str {
        "test_coin_address_withdraw"
    }

    fn operation_flag(&self) -> u32 {
        Self::FLAG
    }

    fn resource_requests(&self) -> Vec<ResourceRequest> {
        vec![ResourceRequest::AddressBalance]
    }

    fn init_requirements(&self) -> Vec<InitRequirement> {
        vec![
            InitRequirement::CreateTestCoinCap,
            InitRequirement::SeedTestCoinAddressBalance,
        ]
    }

    fn apply(
        &self,
        builder: &mut ProgrammableTransactionBuilder,
        resources: &OperationResources,
        rng: &mut SmallRng,
    ) {
        let test_coin_type = resources
            .test_coin_type
            .clone()
            .expect("Test coin type not resolved");

        let amount = if resources.address_balance_amount > 0 {
            resources.address_balance_amount
        } else {
            rng.gen_range(100..500)
        };

        let withdrawal_arg =
            FundsWithdrawalArg::balance_from_sender(amount, test_coin_type.clone());

        let balance = builder.funds_withdrawal(withdrawal_arg).unwrap();

        let coin = builder.programmable_move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            Identifier::new("coin").unwrap(),
            Identifier::new("redeem_funds").unwrap(),
            vec![test_coin_type.clone()],
            vec![balance],
        );

        let recipient = SuiAddress::random_for_testing_only();
        builder.transfer_arg(recipient, coin);
    }
}

pub struct TestCoinObjectWithdraw;

impl TestCoinObjectWithdraw {
    pub const FLAG: u32 = 1 << 10;
    pub const DESCRIPTOR: OperationDescriptor = OperationDescriptor {
        name: "TestCoinObjectWithdraw",
        flag: Self::FLAG,
        factory: || Box::new(TestCoinObjectWithdraw),
    };
}

impl Operation for TestCoinObjectWithdraw {
    fn name(&self) -> &'static str {
        "test_coin_object_withdraw"
    }

    fn operation_flag(&self) -> u32 {
        Self::FLAG
    }

    fn resource_requests(&self) -> Vec<ResourceRequest> {
        vec![ResourceRequest::ObjectBalance]
    }

    fn init_requirements(&self) -> Vec<InitRequirement> {
        vec![InitRequirement::CreateBalancePool]
    }

    fn apply(
        &self,
        builder: &mut ProgrammableTransactionBuilder,
        resources: &OperationResources,
        rng: &mut SmallRng,
    ) {
        let (pool_id, pool_version) = resources.balance_pool.expect("Balance pool not resolved");
        let test_coin_type = resources
            .test_coin_type
            .clone()
            .expect("Test coin type not resolved");

        let amount = if resources.address_balance_amount > 0 {
            resources.address_balance_amount
        } else {
            rng.gen_range(100..1000)
        };

        let pool_arg = builder
            .obj(ObjectArg::SharedObject {
                id: pool_id,
                initial_shared_version: pool_version,
                mutability: SharedObjectMutability::Mutable,
            })
            .unwrap();

        let amount_arg = builder.pure(amount).unwrap();

        let withdrawal = builder.programmable_move_call(
            resources.package_id,
            Identifier::new("balance_pool").unwrap(),
            Identifier::new("withdraw").unwrap(),
            vec![test_coin_type.clone()],
            vec![pool_arg, amount_arg],
        );

        let balance = builder.programmable_move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            Identifier::new("balance").unwrap(),
            Identifier::new("redeem_funds").unwrap(),
            vec![test_coin_type.clone()],
            vec![withdrawal],
        );

        let coin = builder.programmable_move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            Identifier::new("coin").unwrap(),
            Identifier::new("from_balance").unwrap(),
            vec![test_coin_type],
            vec![balance],
        );

        let recipient = SuiAddress::random_for_testing_only();
        builder.transfer_arg(recipient, coin);
    }
}
