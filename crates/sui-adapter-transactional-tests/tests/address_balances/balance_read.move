// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Test settled_funds_value, pending_funds_deposits, and pending_funds_withdrawals from sui::balance

//# init --addresses test=0x0 --accounts A B C D --enable-accumulators --simulator

//# publish
module test::balance_read;

public fun balance_check(acc: &sui::accumulator::AccumulatorRoot, addr: address, expected: u64) {
  let balance = sui::balance::settled_funds_value<sui::sui::SUI>(acc, addr);

  assert!(balance == expected, 100);
}

public fun balance_check_fake(
  acc: &sui::accumulator::AccumulatorRoot,
  addr: address,
  expected: u64,
) {
  let balance = sui::balance::settled_funds_value<u64>(acc, addr);

  assert!(balance == expected, 100);
}

public fun pending_deposits_check(addr: address, expected: u64) {
  let pending = sui::balance::pending_funds_deposits<sui::sui::SUI>(addr);
  assert!(pending == expected, 101);
}

public fun pending_withdrawals_check(addr: address, expected: u64) {
  let pending = sui::balance::pending_funds_withdrawals<sui::sui::SUI>(addr);
  assert!(pending == expected, 102);
}

public fun pending_net_positive_check(
  acc: &sui::accumulator::AccumulatorRoot,
  addr: address,
  expected: u64,
) {
  let pending = sui::balance::positive_pending_funds_value<sui::sui::SUI>(acc, addr);
  assert!(pending.is_some(), 107);
  assert!(pending.destroy_some() == expected, 108);
}

public fun pending_net_positive_none_check(
  acc: &sui::accumulator::AccumulatorRoot,
  addr: address,
) {
  let pending = sui::balance::positive_pending_funds_value<sui::sui::SUI>(acc, addr);
  assert!(pending.is_none(), 109);
}

public fun pending_net_positive_check_fake(
  acc: &sui::accumulator::AccumulatorRoot,
  addr: address,
  expected: u64,
) {
  let pending = sui::balance::positive_pending_funds_value<u64>(acc, addr);
  assert!(pending.is_some(), 110);
  assert!(pending.destroy_some() == expected, 111);
}

// An object we can send funds to and withdraw from
public struct FundedObject has key {
  id: UID,
}

public fun create_funded_object(ctx: &mut TxContext) {
  let obj = FundedObject {
    id: object::new(ctx),
  };
  transfer::share_object(obj);
}

public fun funded_object_address(obj: &FundedObject): address {
  object::uid_to_address(&obj.id)
}

public fun withdraw_and_check_pending(obj: &mut FundedObject, amount: u64) {
  let obj_addr = object::uid_to_address(&obj.id);

  // Check pending withdrawals is 0 before withdrawing
  let pending_before = sui::balance::pending_funds_withdrawals<sui::sui::SUI>(obj_addr);
  assert!(pending_before == 0, 103);

  // Create a withdrawal from the object (this just creates a Withdrawal ticket)
  let withdrawal = sui::balance::withdraw_funds_from_object<sui::sui::SUI>(&mut obj.id, amount);

  // Pending withdrawals is still 0 because the Split event is emitted on redeem
  let pending_after_ticket = sui::balance::pending_funds_withdrawals<sui::sui::SUI>(obj_addr);
  assert!(pending_after_ticket == 0, 104);

  // Redeem the withdrawal - this emits the Split event
  let balance = sui::balance::redeem_funds(withdrawal);

  // Now pending withdrawals should reflect the redeemed amount
  let pending_after_redeem = sui::balance::pending_funds_withdrawals<sui::sui::SUI>(obj_addr);
  assert!(pending_after_redeem == amount, 105);

  // Send the balance somewhere to dispose of it
  sui::balance::send_funds(balance, @0x0);
}

// Withdraw from an object and verify that pending_net_positive returns None (underflow).
// This tests the case where an object has 0 settled funds but we withdraw anyway.
public fun withdraw_and_check_underflow(
  acc: &sui::accumulator::AccumulatorRoot,
  obj: &mut FundedObject,
  amount: u64,
) {
  let obj_addr = object::uid_to_address(&obj.id);

  // Withdraw from the object and redeem
  let withdrawal = sui::balance::withdraw_funds_from_object<sui::sui::SUI>(&mut obj.id, amount);
  let balance = sui::balance::redeem_funds(withdrawal);

  // Check that pending_net_positive returns None (0 settled + 0 deposits - amount = underflow)
  let net = sui::balance::positive_pending_funds_value<sui::sui::SUI>(acc, obj_addr);
  assert!(net.is_none(), 112);

  // Send the balance somewhere to dispose of it
  sui::balance::send_funds(balance, @0x0);
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

//# programmable --sender A --inputs 1042 @B
// Send some SUI from A to B
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::send_funds<sui::sui::SUI>(Result(0), Input(1));

//# create-checkpoint

//# programmable --sender A --inputs immshared(2764) @B 1042
// Check that we can read the balance of B
//> 0: test::balance_read::balance_check(Input(0), Input(1), Input(2));

//# programmable --sender A --inputs immshared(2764) @C 0
// Check that C has a zero balance
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

//# programmable --sender A --inputs immshared(2764) @D 18446744073709551615
// Check that D's balance is clamped to u64::MAX
//> 0: test::balance_read::balance_check_fake(Input(0), Input(1), Input(2));

// Test pending deposits: check that C has 0 pending deposits initially
//# programmable --sender A --inputs @C 0
//> 0: test::balance_read::pending_deposits_check(Input(0), Input(1));

// Test pending deposits: send funds and check that pending deposits is updated within same tx
//# programmable --sender A --inputs 777 @C 777
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::send_funds<sui::sui::SUI>(Result(0), Input(1));
//> 2: test::balance_read::pending_deposits_check(Input(1), Input(2));

// Test pending withdrawals: check that B has 0 pending withdrawals
//# programmable --sender A --inputs @B 0
//> 0: test::balance_read::pending_withdrawals_check(Input(0), Input(1));

// Test pending withdrawals with a withdrawal arg (account balance withdrawal):
// B has 1042 SUI from the earlier send (task 2). Create a withdrawal arg, redeem it,
// and verify pending_withdrawals is updated.
//# programmable --sender B --inputs withdraw<sui::balance::Balance<sui::sui::SUI>>(200) @B 200 0
//> 0: test::balance_read::pending_withdrawals_check(Input(1), Input(3));
//> 1: sui::balance::redeem_funds<sui::sui::SUI>(Input(0));
//> 2: test::balance_read::pending_withdrawals_check(Input(1), Input(2));
//> 3: sui::balance::send_funds<sui::sui::SUI>(Result(1), Input(1));

// Test pending withdrawals with withdraw_funds_from_object (object balance withdrawal):
// First create an object we can fund
//# programmable --sender A
//> 0: test::balance_read::create_funded_object();

// Send funds to the object's address
//# programmable --sender A --inputs 500 object(15,0)
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: test::balance_read::funded_object_address(Input(1));
//> 2: sui::coin::send_funds<sui::sui::SUI>(Result(0), Result(1));

//# create-checkpoint

// Now withdraw from the object and check that pending_withdrawals is updated
//# programmable --sender A --inputs object(15,0) 333
//> 0: test::balance_read::withdraw_and_check_pending(Input(0), Input(1));

// Test positive_pending_funds_value:
// Case 1: C has 777 settled (from earlier deposit in task 12). Send 100 more.
// Expected: settled(777) + deposits(100) - withdrawals(0) = 877
//# programmable --sender A --inputs immshared(2764) 100 @C 877
//> 0: SplitCoins(Gas, [Input(1)]);
//> 1: sui::coin::send_funds<sui::sui::SUI>(Result(0), Input(2));
//> 2: test::balance_read::pending_net_positive_check(Input(0), Input(2), Input(3));

// Case 2: withdrawals > settled - should return None
// Create a new object and withdraw from it. The object has 0 settled funds,
// so any withdrawal causes underflow: settled(0) + deposits(0) - withdrawals(100) = -100
//# programmable --sender A
//> 0: test::balance_read::create_funded_object();

//# programmable --sender A --inputs immshared(2764) object(20,0) 100
//> 0: test::balance_read::withdraw_and_check_underflow(Input(0), Input(1), Input(2));

// Case 3: settled + deposits = withdrawals - should return Some(0)
// D has u64::MAX - 1 + u64::MAX - 1 settled (clamped to u64::MAX).
// Withdraw u64::MAX, deposit 0. Expected: u64::MAX - u64::MAX = 0
//# programmable --sender D --inputs immshared(2764) withdraw<sui::balance::Balance<u64>>(18446744073709551615) @D 0
//> 0: sui::balance::redeem_funds<u64>(Input(1));
//> 1: test::balance_read::pending_net_positive_check_fake(Input(0), Input(2), Input(3));
//> 2: sui::balance::send_funds<u64>(Result(0), Input(2));
