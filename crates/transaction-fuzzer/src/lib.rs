// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod account_universe;
pub mod config_fuzzer;
pub mod executor;
pub mod transaction_data_gen;
pub mod type_arg_fuzzer;

use executor::Executor;
use proptest::collection::vec;
use proptest::test_runner::TestRunner;
use std::fmt::Debug;
use sui_protocol_config::ProtocolConfig;
use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::crypto::get_key_pair;
use sui_types::crypto::AccountKeyPair;
use sui_types::digests::TransactionDigest;
use sui_types::object::{MoveObject, Object, Owner, OBJECT_START_VERSION};
use sui_types::{gas_coin::TOTAL_SUPPLY_MIST, messages::GasData};

use proptest::prelude::*;
use rand::{rngs::StdRng, SeedableRng};

fn new_gas_coin_with_balance_and_owner(balance: u64, owner: Owner) -> Object {
    Object::new_move(
        MoveObject::new_gas_coin(OBJECT_START_VERSION, ObjectID::random(), balance),
        owner,
        TransactionDigest::genesis(),
    )
}

/// Given a list of gas coin owners, generate random gas data and gas coins
/// with the given owners.
fn generate_random_gas_data(
    seed: [u8; 32],
    gas_coin_owners: Vec<Owner>, // arbitrarily generated owners, can be shared or immutable or obj-owned too
    owned_by_sender: bool,       // whether to set owned gas coins to be owned by the sender
) -> GasDataWithObjects {
    let (sender, sender_key): (SuiAddress, AccountKeyPair) = get_key_pair();
    let mut rng = StdRng::from_seed(seed);
    let mut gas_objects = vec![];
    let mut object_refs = vec![];

    let max_gas_balance = TOTAL_SUPPLY_MIST;

    let total_gas_balance = rng.gen_range(0..=max_gas_balance);
    let mut remaining_gas_balance = total_gas_balance;
    let num_gas_objects = gas_coin_owners.len();
    let gas_coin_owners = gas_coin_owners
        .iter()
        .map(|o| match o {
            Owner::ObjectOwner(_) | Owner::AddressOwner(_) if owned_by_sender => {
                Owner::AddressOwner(sender)
            }
            _ => *o,
        })
        .collect::<Vec<_>>();
    for owner in gas_coin_owners.iter().take(num_gas_objects - 1) {
        let gas_balance = rng.gen_range(0..=remaining_gas_balance);
        let gas_object = new_gas_coin_with_balance_and_owner(gas_balance, *owner);
        remaining_gas_balance -= gas_balance;
        object_refs.push(gas_object.compute_object_reference());
        gas_objects.push(gas_object);
    }
    // Put the remaining balance in the last gas object.
    let last_gas_object = new_gas_coin_with_balance_and_owner(
        remaining_gas_balance,
        gas_coin_owners[num_gas_objects - 1],
    );
    object_refs.push(last_gas_object.compute_object_reference());
    gas_objects.push(last_gas_object);

    assert_eq!(gas_objects.len(), num_gas_objects);
    assert_eq!(
        gas_objects
            .iter()
            .map(|o| o.data.try_as_move().unwrap().get_coin_value_unsafe())
            .sum::<u64>(),
        total_gas_balance
    );

    GasDataWithObjects {
        gas_data: GasData {
            payment: object_refs,
            owner: sender,
            price: rng.gen_range(0..=ProtocolConfig::get_for_max_version().max_gas_price()),
            budget: rng.gen_range(0..=ProtocolConfig::get_for_max_version().max_tx_gas()),
        },
        objects: gas_objects,
        sender_key,
    }
}

/// Need to have a wrapper struct here so we can implement Arbitrary for it.
#[derive(Debug)]
pub struct GasDataWithObjects {
    pub gas_data: GasData,
    pub sender_key: AccountKeyPair,
    pub objects: Vec<Object>,
}

#[derive(Debug, Default)]
pub struct GasDataGenConfig {
    pub max_num_gas_objects: usize,
    pub owned_by_sender: bool,
}

impl GasDataGenConfig {
    pub fn owned_by_sender_or_immut() -> Self {
        Self {
            max_num_gas_objects: ProtocolConfig::get_for_max_version().max_gas_payment_objects()
                as usize,
            owned_by_sender: true,
        }
    }

    pub fn any_owner() -> Self {
        Self {
            max_num_gas_objects: ProtocolConfig::get_for_max_version().max_gas_payment_objects()
                as usize,
            owned_by_sender: false,
        }
    }
}

impl proptest::arbitrary::Arbitrary for GasDataWithObjects {
    type Parameters = GasDataGenConfig;
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(params: Self::Parameters) -> Self::Strategy {
        (
            any::<[u8; 32]>(),
            vec(any::<Owner>(), 1..=params.max_num_gas_objects),
        )
            .prop_map(move |(seed, owners)| {
                generate_random_gas_data(seed, owners, params.owned_by_sender)
            })
            .boxed()
    }
}

#[derive(Clone, Debug)]
pub struct TestData<D> {
    pub data: D,
    pub executor: Executor,
}

/// Run a proptest test with give number of test cases, a strategy for something and a test function testing that something
/// with an `Arc<AuthorityState>`.
pub fn run_proptest<D>(
    num_test_cases: u32,
    strategy: impl Strategy<Value = D>,
    test_fn: impl Fn(D, Executor) -> Result<(), TestCaseError>,
) where
    D: Debug + 'static,
{
    let mut runner = TestRunner::new(ProptestConfig {
        cases: num_test_cases,
        ..Default::default()
    });
    let executor = Executor::new();
    let strategy_with_authority = strategy.prop_map(|data| TestData {
        data,
        executor: executor.clone(),
    });
    let result = runner.run(&strategy_with_authority, |test_data| {
        test_fn(test_data.data, test_data.executor)
    });
    if result.is_err() {
        panic!("test failed: {:?}", result);
    }
}
