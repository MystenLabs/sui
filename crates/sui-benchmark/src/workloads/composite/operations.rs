// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use mysten_common::random::get_rng;
use rand::Rng;
use sui_types::TypeTag;
use sui_types::base_types::{ObjectID, SequenceNumber, SuiAddress};
use sui_types::gas_coin::GAS;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::{
    Argument, CallArg, Command, FundsWithdrawalArg, ObjectArg, SharedObjectMutability,
};
use sui_types::{
    Identifier, SUI_ACCUMULATOR_ROOT_OBJECT_ID, SUI_FRAMEWORK_PACKAGE_ID,
    SUI_RANDOMNESS_STATE_OBJECT_ID,
};

use super::AccountState;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum InitRequirement {
    SeedAddressBalance,
    CreateBalancePool,
    SeedBalancePool,
    CreateTestCoinCap,
    SeedTestCoinAddressBalance,
    EnableAddressAlias,
}

pub const ALIAS_TX: &str = "alias_tx";
pub const ALIAS_REMOVE: &str = "alias_remove";
pub const ALIAS_ADD: &str = "alias_add";
pub const INVALID_ALIAS_TX: &str = "invalid_alias_tx";

pub struct OperationDescriptor {
    pub name: &'static str,
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
    AddressBalanceOverdraw::DESCRIPTOR,
    AccumulatorBalanceRead::DESCRIPTOR,
    ObjectBalanceOverdraw::DESCRIPTOR,
    AuthenticatedEventEmit::DESCRIPTOR,
];

#[derive(Debug, Clone)]
pub enum ResourceRequest {
    SharedCounter,
    Randomness,
    AddressBalance,
    ObjectBalance,
    TestCoinCap,
    AccumulatorRoot,
}

#[derive(Debug, Clone, Default)]
pub struct OperationConstraints {
    pub must_be_last_shared_access: bool,
}

pub struct OperationResources {
    pub counter: Option<(ObjectID, SequenceNumber)>,
    pub randomness: Option<SequenceNumber>,
    pub accumulator_root: Option<SequenceNumber>,
    pub package_id: ObjectID,
    pub address_balance_amount: u64,
    pub balance_pool: Option<(ObjectID, SequenceNumber)>,
    pub test_coin_cap: Option<(ObjectID, SequenceNumber)>,
    pub test_coin_type: Option<TypeTag>,
}

pub trait Operation: Send + Sync {
    fn name(&self) -> &'static str;
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
        account_state: &AccountState,
    );
}

pub struct SharedCounterIncrement;

impl SharedCounterIncrement {
    pub const NAME: &'static str = "shared_counter_increment";
    pub const DESCRIPTOR: OperationDescriptor = OperationDescriptor {
        name: Self::NAME,
        factory: || Box::new(SharedCounterIncrement),
    };
}

impl Operation for SharedCounterIncrement {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn resource_requests(&self) -> Vec<ResourceRequest> {
        vec![ResourceRequest::SharedCounter]
    }

    fn apply(
        &self,
        builder: &mut ProgrammableTransactionBuilder,
        resources: &OperationResources,
        _account_state: &AccountState,
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
    pub const NAME: &'static str = "shared_counter_read";
    pub const DESCRIPTOR: OperationDescriptor = OperationDescriptor {
        name: Self::NAME,
        factory: || Box::new(SharedCounterRead),
    };
}

impl Operation for SharedCounterRead {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn resource_requests(&self) -> Vec<ResourceRequest> {
        vec![ResourceRequest::SharedCounter]
    }

    fn apply(
        &self,
        builder: &mut ProgrammableTransactionBuilder,
        resources: &OperationResources,
        _account_state: &AccountState,
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
    pub const NAME: &'static str = "randomness_read";
    pub const DESCRIPTOR: OperationDescriptor = OperationDescriptor {
        name: Self::NAME,
        factory: || Box::new(RandomnessRead),
    };
}

impl Operation for RandomnessRead {
    fn name(&self) -> &'static str {
        Self::NAME
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
        _account_state: &AccountState,
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
    pub const NAME: &'static str = "address_balance_deposit";
    pub const DESCRIPTOR: OperationDescriptor = OperationDescriptor {
        name: Self::NAME,
        factory: || Box::new(AddressBalanceDeposit),
    };
}

impl Operation for AddressBalanceDeposit {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn resource_requests(&self) -> Vec<ResourceRequest> {
        vec![ResourceRequest::AddressBalance]
    }

    fn apply(
        &self,
        builder: &mut ProgrammableTransactionBuilder,
        resources: &OperationResources,
        _account_state: &AccountState,
    ) {
        let recipient = SuiAddress::random_for_testing_only();

        let amount = if resources.address_balance_amount > 0 {
            resources.address_balance_amount
        } else {
            get_rng().gen_range(1000..10000)
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
    pub const NAME: &'static str = "address_balance_withdraw";
    pub const DESCRIPTOR: OperationDescriptor = OperationDescriptor {
        name: Self::NAME,
        factory: || Box::new(AddressBalanceWithdraw),
    };
}

impl Operation for AddressBalanceWithdraw {
    fn name(&self) -> &'static str {
        Self::NAME
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
        _account_state: &AccountState,
    ) {
        let amount = if resources.address_balance_amount > 0 {
            resources.address_balance_amount
        } else {
            get_rng().gen_range(100..1000)
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
    pub const NAME: &'static str = "object_balance_deposit";
    pub const DESCRIPTOR: OperationDescriptor = OperationDescriptor {
        name: Self::NAME,
        factory: || Box::new(ObjectBalanceDeposit),
    };
}

impl Operation for ObjectBalanceDeposit {
    fn name(&self) -> &'static str {
        Self::NAME
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
        _account_state: &AccountState,
    ) {
        let (pool_id, initial_shared_version) =
            resources.balance_pool.expect("Balance pool not resolved");

        let amount = if resources.address_balance_amount > 0 {
            resources.address_balance_amount
        } else {
            get_rng().gen_range(1000..10000)
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
    pub const NAME: &'static str = "object_balance_withdraw";
    pub const DESCRIPTOR: OperationDescriptor = OperationDescriptor {
        name: Self::NAME,
        factory: || Box::new(ObjectBalanceWithdraw),
    };
}

impl Operation for ObjectBalanceWithdraw {
    fn name(&self) -> &'static str {
        Self::NAME
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
        _account_state: &AccountState,
    ) {
        let (pool_id, initial_shared_version) =
            resources.balance_pool.expect("Balance pool not resolved");

        let amount = if resources.address_balance_amount > 0 {
            resources.address_balance_amount
        } else {
            get_rng().gen_range(100..1000)
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
    pub const NAME: &'static str = "test_coin_mint";
    pub const DESCRIPTOR: OperationDescriptor = OperationDescriptor {
        name: Self::NAME,
        factory: || Box::new(TestCoinMint),
    };
}

impl Operation for TestCoinMint {
    fn name(&self) -> &'static str {
        Self::NAME
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
        _account_state: &AccountState,
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
            get_rng().gen_range(1000..10000)
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
    pub const NAME: &'static str = "test_coin_address_deposit";
    pub const DESCRIPTOR: OperationDescriptor = OperationDescriptor {
        name: Self::NAME,
        factory: || Box::new(TestCoinAddressDeposit),
    };
}

impl Operation for TestCoinAddressDeposit {
    fn name(&self) -> &'static str {
        Self::NAME
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
        _account_state: &AccountState,
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
            get_rng().gen_range(100..1000)
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
    pub const NAME: &'static str = "test_coin_address_withdraw";
    pub const DESCRIPTOR: OperationDescriptor = OperationDescriptor {
        name: Self::NAME,
        factory: || Box::new(TestCoinAddressWithdraw),
    };
}

impl Operation for TestCoinAddressWithdraw {
    fn name(&self) -> &'static str {
        Self::NAME
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
        _account_state: &AccountState,
    ) {
        let test_coin_type = resources
            .test_coin_type
            .clone()
            .expect("Test coin type not resolved");

        let amount = if resources.address_balance_amount > 0 {
            resources.address_balance_amount
        } else {
            get_rng().gen_range(100..500)
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
    pub const NAME: &'static str = "test_coin_object_withdraw";
    pub const DESCRIPTOR: OperationDescriptor = OperationDescriptor {
        name: Self::NAME,
        factory: || Box::new(TestCoinObjectWithdraw),
    };
}

impl Operation for TestCoinObjectWithdraw {
    fn name(&self) -> &'static str {
        Self::NAME
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
        _account_state: &AccountState,
    ) {
        let (pool_id, pool_version) = resources.balance_pool.expect("Balance pool not resolved");
        let test_coin_type = resources
            .test_coin_type
            .clone()
            .expect("Test coin type not resolved");

        let amount = if resources.address_balance_amount > 0 {
            resources.address_balance_amount
        } else {
            get_rng().gen_range(100..1000)
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

pub struct AddressBalanceOverdraw;

impl AddressBalanceOverdraw {
    pub const NAME: &'static str = "address_balance_overdraw";
    pub const DESCRIPTOR: OperationDescriptor = OperationDescriptor {
        name: Self::NAME,
        factory: || Box::new(AddressBalanceOverdraw),
    };
}

impl Operation for AddressBalanceOverdraw {
    fn name(&self) -> &'static str {
        Self::NAME
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
        _resources: &OperationResources,
        account_state: &AccountState,
    ) {
        let withdraw_amount = if account_state.sui_balance == 0 {
            // reservations of zero are invalid, so the transaction will be rejected.
            0
        } else {
            // withdraw at least half the balance
            let half_balance = std::cmp::max(1, account_state.sui_balance / 2);
            get_rng().gen_range(half_balance..=account_state.sui_balance)
        };

        let withdrawal = FundsWithdrawalArg::balance_from_sender(withdraw_amount, GAS::type_tag());

        let withdraw_arg = builder.funds_withdrawal(withdrawal).unwrap();
        let recipient = builder.pure(account_state.sender).unwrap();

        let balance = builder.programmable_move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            Identifier::new("balance").unwrap(),
            Identifier::new("redeem_funds").unwrap(),
            vec![GAS::type_tag()],
            vec![withdraw_arg],
        );

        builder.programmable_move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            Identifier::new("balance").unwrap(),
            Identifier::new("send_funds").unwrap(),
            vec![GAS::type_tag()],
            vec![balance, recipient],
        );
    }
}

pub struct AccumulatorBalanceRead;

impl AccumulatorBalanceRead {
    pub const NAME: &'static str = "accumulator_balance_read";
    pub const DESCRIPTOR: OperationDescriptor = OperationDescriptor {
        name: Self::NAME,
        factory: || Box::new(AccumulatorBalanceRead),
    };
}

impl Operation for AccumulatorBalanceRead {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn resource_requests(&self) -> Vec<ResourceRequest> {
        vec![ResourceRequest::AccumulatorRoot]
    }

    fn init_requirements(&self) -> Vec<InitRequirement> {
        vec![InitRequirement::SeedAddressBalance]
    }

    fn apply(
        &self,
        builder: &mut ProgrammableTransactionBuilder,
        resources: &OperationResources,
        account_state: &AccountState,
    ) {
        let initial_shared_version = resources
            .accumulator_root
            .expect("AccumulatorRoot not resolved");

        let root_arg = builder
            .obj(ObjectArg::SharedObject {
                id: SUI_ACCUMULATOR_ROOT_OBJECT_ID,
                initial_shared_version,
                mutability: SharedObjectMutability::Immutable,
            })
            .unwrap();

        let addr_arg = builder.pure(account_state.sender).unwrap();

        builder.programmable_move_call(
            resources.package_id,
            Identifier::new("accumulator_read").unwrap(),
            Identifier::new("read_settled_balance").unwrap(),
            vec![],
            vec![root_arg, addr_arg],
        );
    }
}

pub struct ObjectBalanceOverdraw;

impl ObjectBalanceOverdraw {
    pub const NAME: &'static str = "object_balance_overdraw";
    pub const DESCRIPTOR: OperationDescriptor = OperationDescriptor {
        name: Self::NAME,
        factory: || Box::new(ObjectBalanceOverdraw),
    };
}

impl Operation for ObjectBalanceOverdraw {
    fn name(&self) -> &'static str {
        Self::NAME
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
        account_state: &AccountState,
    ) {
        let (pool_id, initial_shared_version) =
            resources.balance_pool.expect("Balance pool not resolved");

        let withdraw_amount = if account_state.pool_balance == 0 {
            0
        } else {
            let half_balance = std::cmp::max(1, account_state.pool_balance / 2);
            get_rng().gen_range(half_balance..=account_state.pool_balance)
        };

        let pool_arg = builder
            .obj(ObjectArg::SharedObject {
                id: pool_id,
                initial_shared_version,
                mutability: SharedObjectMutability::Mutable,
            })
            .unwrap();

        let amount_arg = builder.pure(withdraw_amount).unwrap();

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

pub struct AuthenticatedEventEmit;

impl AuthenticatedEventEmit {
    pub const NAME: &'static str = "authenticated_event_emit";
    pub const DESCRIPTOR: OperationDescriptor = OperationDescriptor {
        name: Self::NAME,
        factory: || Box::new(AuthenticatedEventEmit),
    };
}

impl Operation for AuthenticatedEventEmit {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn resource_requests(&self) -> Vec<ResourceRequest> {
        vec![]
    }

    fn apply(
        &self,
        builder: &mut ProgrammableTransactionBuilder,
        resources: &OperationResources,
        _account_state: &AccountState,
    ) {
        let mut rng = get_rng();
        let count: u64 = rng.gen_range(1..=5);

        let start_arg = builder.pure(count).unwrap();
        let count_arg = builder.pure(count).unwrap();

        builder.programmable_move_call(
            resources.package_id,
            Identifier::new("auth_event").unwrap(),
            Identifier::new("emit_multiple").unwrap(),
            vec![],
            vec![start_arg, count_arg],
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_operation_names_are_unique() {
        let mut names = std::collections::HashSet::new();
        for desc in ALL_OPERATIONS {
            assert!(
                names.insert(desc.name),
                "Duplicate operation name: {}",
                desc.name
            );
        }
    }
}
