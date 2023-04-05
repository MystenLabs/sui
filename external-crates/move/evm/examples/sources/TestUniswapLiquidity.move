// This module is to test the interaction with the existing dapp called UniswapV2.
module Evm::TestUniswapLiquidity {
    use Evm::IERC20;
    use Evm::IUniswapV2Router;
    use Evm::IUniswapV2Factory;
    use Evm::U256::{Self, U256, u256_from_words};
    use Evm::Evm::{sender, self, block_timestamp};

    public fun addLiquidity(
        tokenA: address,
        tokenB: address,
        amountA: U256,
        amountB: U256,
    ) {
        // TODO: Replace these local constants with module-level constants once Move supports 20-bytes addresses and literals.
        let const_ROUTER = U256::to_address(u256_from_words(0x7a250d56, 0x30B4cF539739dF2C5dAcb4c659F2488D));

        IERC20::call_transferFrom(tokenA, sender(), self(), copy amountA);
        IERC20::call_transferFrom(tokenB, sender(), self(), copy amountB);

        IERC20::call_approve(tokenA, const_ROUTER, copy amountA);
        IERC20::call_approve(tokenB, const_ROUTER, copy amountB);

        let (_amountA, _amountB, _liquidity) = IUniswapV2Router::call_addLiquidity(
            const_ROUTER,
            tokenA,
            tokenB,
            amountA,
            amountB,
            U256::one(),
            U256::one(),
            self(),
            block_timestamp()
        );
    }

    public fun removeLiquidity(
        tokenA: address,
        tokenB: address,
    ) {
        // TODO: Replace these local constants with module-level constants once Move supports 20-bytes addresses and literals.
        let const_FACTORY = U256::to_address(u256_from_words(0x5C69bEe7, 0x01ef814a2B6a3EDD4B1652CB9cc5aA6f));
        let const_ROUTER = U256::to_address(u256_from_words(0x7a250d56, 0x30B4cF539739dF2C5dAcb4c659F2488D));

        let pair = IUniswapV2Factory::call_getPair(const_FACTORY, tokenA, tokenB);

        let liquidity = IERC20::call_balanceOf(pair, self());
        IERC20::call_approve(pair, const_ROUTER, copy liquidity);

        let (_amountA, _amountB) = IUniswapV2Router::call_removeLiquidity(
            const_ROUTER,
            tokenA,
            tokenB,
            liquidity,
            U256::one(),
            U256::one(),
            self(),
            block_timestamp()
        );
    }
}
