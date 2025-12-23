// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Test read_settled_address_balance from sui::balance

//# init --addresses test=0x0 --accounts A B C D --enable-accumulators --simulator

//# publish
module test::balance_read;

public fun balance_check(acc: &sui::accumulator::AccumulatorRoot, addr: address, expected: u64) {
  let balance = sui::balance::read_settled_address_balance<sui::sui::SUI>(acc, addr);

  assert!(balance == expected, 100);
}

public fun balance_check_fake(
  acc: &sui::accumulator::AccumulatorRoot,
  addr: address,
  expected: u64,
) {
  let balance = sui::balance::read_settled_address_balance<u64>(acc, addr);

  assert!(balance == expected, 100);
}

// a struct to attach Supply objects to to dispose of them.
public struct SupplyHolder has key {
  id: UID,
}

public fun create_supply_holder(ctx: &mut TxContext) {
  let holder = SupplyHolder {
    id: object::new(ctx),
  };
  transfer::share_object(holder);
}

// Send the maximum possible balance to an address so that we can overflow its accumulator quickly.
public fun send_max(holder: &mut SupplyHolder, key: u64, recipient: address) {
  let increase = std::u64::max_value!() - 1;

  // make a "fake" supply and increase it by u64::MAX - 1
  let mut supply = sui::balance::create_supply<u64>(0);
  let balance = supply.increase_supply(increase);
  sui::balance::send_funds(balance, recipient);
  sui::dynamic_field::add(&mut holder.id, key, supply);
}

// Send some SUI from A to B
//# programmable --sender A --inputs 1042 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::send_funds<sui::sui::SUI>(Result(0), Input(1));

//# create-checkpoint

// Check that we can read the balance of B
//# programmable --sender A --inputs immshared(2764) @B 1042
//> 0: test::balance_read::balance_check(Input(0), Input(1), Input(2));

// Check that C has a zero balance
//# programmable --sender A --inputs immshared(2764) @C 0
//> 0: test::balance_read::balance_check(Input(0), Input(1), Input(2));

// Overflow D's balance (requires two transactions, since an individual transaction
// will abort on overflow)
//
//# programmable --sender A --inputs @A
//> 0: test::balance_read::create_supply_holder();
//# programmable --sender A --inputs object(6,0) 0 @D
//> 0: test::balance_read::send_max(Input(0), Input(1), Input(2));
//# programmable --sender A --inputs object(6,0) 1 @D
//> 0: test::balance_read::send_max(Input(0), Input(1), Input(2));

//# create-checkpoint

// Check that D's balance is clamped to u64::MAX
//# programmable --sender A --inputs immshared(2764) @D 18446744073709551615
//> 0: test::balance_read::balance_check_fake(Input(0), Input(1), Input(2));
