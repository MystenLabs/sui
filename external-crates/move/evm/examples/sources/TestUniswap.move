// This module is to test the interaction with the existing dapp called UniswapV2.
module Evm::TestUniswap {
    use Evm::IERC20;
    use Evm::IUniswapV2Router;
    use Evm::U256::{Self, U256, u256_from_words};
    use Evm::Evm::{sender, self, block_timestamp};
    use std::vector;

    // Swap tokenIn for tokenOut.
    public fun swap(
        tokenIn: address,
        tokenOut: address,
        amountIn: U256,
        amountOutMin: U256,
        to: address
    ) {
        // TODO: Replace these local constants with module-level constants once Move supports 20-bytes addresses and literals.
        let const_UNISWAP_V2_ROUTER = U256::to_address(u256_from_words(0x7a250d56, 0x30B4cF539739dF2C5dAcb4c659F2488D));
        let const_WETH = U256::to_address(u256_from_words(0xC02aaA39, 0xb223FE8D0A0e5C4F27eAD9083C756Cc2));

        IERC20::call_transferFrom(tokenIn, sender(), self(), copy amountIn);
        IERC20::call_approve(tokenIn, const_UNISWAP_V2_ROUTER, copy amountIn);

        let path = vector::empty<address>();

        if(tokenIn == const_WETH || tokenOut == const_WETH) {
            // Directly swapping tokenIn for tokenOut.
            vector::push_back(&mut path, tokenIn);
            vector::push_back(&mut path, tokenOut);
        }
        else {
            // Swapping tokenIn for WETH, and then WETH for tokenOut.
            // Bridging is needed because UniswapV2 cannot directly swap two ERC20 token types.
            vector::push_back(&mut path, tokenIn);
            vector::push_back(&mut path, const_WETH);
            vector::push_back(&mut path, tokenOut);
        };

        IUniswapV2Router::call_swapExactTokensForTokens(
            const_UNISWAP_V2_ROUTER,
            amountIn,
            amountOutMin,
            path,
            to,
            block_timestamp()
        );
    }
}
