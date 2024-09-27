// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title IBridgeLimiter
/// @notice Interface for the BridgeLimiter contract.
interface IBridgeLimiter {
    /// @notice Updates the bridge transfers for a specific token ID and amount. Only the contract
    /// owner can call this function (intended to be the SuiBridge contract).
    /// @dev The amount must be greater than 0 and must not exceed the rolling window limit.
    /// @param chainID The ID of the chain to record the transfer for.
    /// @param tokenID The ID of the token.
    /// @param amount The amount of tokens to be transferred.
    function recordBridgeTransfers(uint8 chainID, uint8 tokenID, uint256 amount) external;

    /// @notice Returns whether the total amount, including the given token amount, will exceed the totalLimit.
    /// @dev The function will calculate the given token amount in USD.
    /// @param chainID The ID of the chain to check limit for.
    /// @param tokenID The ID of the token.
    /// @param amount The amount of the token.
    /// @return boolean indicating whether the total amount will exceed the limit.
    function willAmountExceedLimit(uint8 chainID, uint8 tokenID, uint256 amount)
        external
        view
        returns (bool);

    // We no longer emit this event but keep it here for ABI compatibility.
    /// @dev (deprecated, not in use) Emitted when the hourly transfer amount is updated.
    /// @param hourUpdated The hour that was updated.
    /// @param amount The amount in USD transferred.
    event HourlyTransferAmountUpdated(uint32 hourUpdated, uint256 amount);

    /// @dev Emitted when the total limit is updated.
    /// @param nonce The governance action nonce.
    /// @param sourceChainID The ID of the source chain.
    /// @param newLimit The new limit in USD with 4 decimal places (e.g. 10000 -> $1)
    event LimitUpdatedV2(uint64 nonce, uint8 sourceChainID, uint64 newLimit);

    /// @dev (deprecated in favor of LimitUpdatedV2)
    event LimitUpdated(uint8 sourceChainID, uint64 newLimit);
}
