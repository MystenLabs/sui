#[interface]
/// The interface for UniswapV2Factory.
/// This module defines the API for the interface of UniswapV2Factory and
/// the utility functions such as selectors and `interfaceId`.
module Evm::IUniswapV2Factory {

    #[external(sig=b"getPair(address,address) returns (address)")]
    public native fun call_getPair(contract: address, tokenA: address, tokenB: address): address;

    #[interface_id]
    /// Return the interface identifier.
    // TODO: complete this function.
   public native fun interfaceId(): vector<u8>;

    // TODO: complete this module.
}
