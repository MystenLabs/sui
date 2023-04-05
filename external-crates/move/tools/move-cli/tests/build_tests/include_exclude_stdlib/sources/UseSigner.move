#[evm_contract] // for passing evm test flavor
module 0x1::Example {
  use std::signer;

  public fun f(account: &signer): address {
    signer::address_of(account)
  }
}
