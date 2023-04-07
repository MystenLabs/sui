#[interface]
/// The interface for UniswapV2Router.
/// This module defines the API for the interface of UniswapV2Router and
/// the utility functions such as selectors and `interfaceId`.
module Evm::IUniswapV2Router {
    use Evm::U256::{U256};

    #[external(sig=b"swapExactTokensForTokens(uint,uint,address[],address,uint) returns (uint[])")]
    public native fun call_swapExactTokensForTokens(contract: address, amountIn: U256, amountOutMin: U256, path: vector<address>, to: address, deadline: U256): vector<U256>;

    #[external(sig=b"addLiquidity(address,address,uint,uint,uint,uint,address) returns (uint,uint,uint)")]
    public native fun call_addLiquidity(contract: address, tokenA: address, tokenB: address, amountADesired: U256, amountBDesired: U256, amountAMin: U256, amountBMin: U256, to: address, deadline: U256): (U256, U256, U256);

    #[external(sig=b"removeLiquidity(address,address,uint,uint,uint,address,uint) returns (uint,uint)")]
    public native fun call_removeLiquidity(contract: address, tokenA: address, tokenB: address, liquidity: U256, amountAMin: U256, amountBMin: U256, to: address, deadline: U256): (U256, U256);

    #[interface_id]
    /// Return the interface identifier.
    // TODO: complete this function.
   public native fun interfaceId(): vector<u8>;

    // TODO: complete this module.
}
