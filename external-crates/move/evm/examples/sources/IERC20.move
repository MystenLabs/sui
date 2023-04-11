#[interface]
/// The interface for ERC-20.
/// This module defines the API for the interface of ERC-20 and
/// the utility functions such as selectors and `interfaceId`.
module Evm::IERC20 {
    use Evm::U256::{U256};

    #[external(sig=b"transferFrom(address,address,uint) returns (bool)")]
    public native fun call_transferFrom(contract: address, from: address, to: address, amount: U256): bool;

    #[external(sig=b"approve(address,uint) returns (bool)")]
    public native fun call_approve(contract: address, spender: address, amount: U256): bool;

    #[external(sig=b"balanceOf(address) returns (uint)")]
    public native fun call_balanceOf(contract: address, account: address): U256;

    #[interface_id]
    /// Return the interface identifier.
    // TODO: complete this function.
   public native fun interfaceId(): vector<u8>;

    // TODO: complete this module.
}
