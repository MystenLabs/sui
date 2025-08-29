// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title IBridgeLimiter
/// @notice Interface for the BridgeLimiter contract.
interface IBridgeLimiterV2 {
    /// @notice Returns whether the total amount, including the given token amount, will exceed the totalLimit.
    /// @dev The function will calculate the given token amount in USD.
    /// @param chainID The ID of the chain to check limit for.
    /// @param tokenID The ID of the token.
    /// @param amount The amount of the token.
    /// @return boolean indicating whether the total amount will exceed the limit.
    function willAmountExceedLimit(uint8 chainID, uint64 nonce, uint8 tokenID, uint256 amount)
        external
        view
        returns (bool);

    event MinLimitNonceUpdated(uint8 sourceChainID, uint64 newLimit);
}
