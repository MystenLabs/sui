// To fix all verification errors, activate lines with MISSING tag
module 0x42::Account {
  struct Account has key {
    balance: u64,
  }

  fun withdraw(account: address, amount: u64) acquires Account {
    // assert!(amount <= AccountLimits::max_decrease(), Errors::invalid_argument()); // MISSING
    let balance = &mut borrow_global_mut<Account>(account).balance;
    assert!(*balance >= amount, errors::limit_exceeded());
    // assert!(*balance - amount >= AccountLimits::min_balance(), Errors::invalid_argument()); // MISSING
    *balance = *balance - amount;
  }

  fun deposit(account: address, amount: u64) acquires Account {
    let balance = &mut borrow_global_mut<Account>(account).balance;
    assert!(*balance <= Limits::max_u64() - amount, errors::limit_exceeded());
    *balance = *balance + amount;
  }

  public(script) fun transfer(from: &signer, to: address, amount: u64) acquires Account {
    assert!(signer::address_of(from) != to, errors::invalid_argument());
    withdraw(signer::address_of(from), amount);
    deposit(to, amount);
  }

  spec transfer {
    let from_addr = signer::address_of(from);
    aborts_if from_addr == to;
    aborts_if bal(from_addr) < amount;
    aborts_if bal(to) + amount > Limits::max_u64();
    ensures bal(from_addr) == old(bal(from_addr)) - amount;
    ensures bal(to) == old(bal(to)) + amount;
    // aborts_if !exists<Account>(from_addr); // MISSING
    // aborts_if !exists<Account>(to); // MISSING
    // aborts_if amount > AccountLimits::max_decrease(); // MISSING
    // aborts_if bal(from_addr) - amount < AccountLimits::min_balance(); // MISSING
  }

  spec fun bal(acc: address): u64 {
    global<Account>(acc).balance
  }

  invariant forall acc: address where exists<Account>(acc):
    bal(acc) >= AccountLimits::min_balance();

  invariant update forall acc: address where exists<Account>(acc):
    old(bal(acc)) - bal(acc) <= AccountLimits::max_decrease();

  use 0x42::Errors;
  use 0x42::Limits;
  use 0x42::AccountLimits;
  use std::signer;
}

module 0x42::Errors {
    public fun limit_exceeded(): u64 { 1 }
    public fun invalid_argument(): u64 { 2 }
}

module 0x42::Limits {
    public fun max_u64(): u64 { 18446744073709551615 }
}
module 0x42::AccountLimits {
    public fun min_balance(): u64 { 5 }
    public fun max_decrease(): u64 { 10 }
}
