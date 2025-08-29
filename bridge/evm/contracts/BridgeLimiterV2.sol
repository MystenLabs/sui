// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "./BridgeLimiter.sol";
import "./interfaces/IBridgeLimiterV2.sol";
import "./utils/BridgeUtilsV2.sol";

/// @title BridgeLimiter
/// @notice A contract that limits the amount of tokens that can be bridged from a given chain within
/// a rolling 24-hour window. This is accomplished by storing the amount bridged from a given chain in USD
/// within a given hourly timestamp. It also provides functions to update the token prices and the total
/// limit of the given chainID measured in USD with 8 decimal precision.
/// The contract is intended to be used and owned by the SuiBridge contract.
contract BridgeLimiterV2 is BridgeLimiter, IBridgeLimiterV2 {
    /* ========== STATE VARIABLES ========== */

    // Minimum nonce for which the limiter is applied; nonces below this value are not limited
    mapping(uint8 chainID => uint64 minLimitedNonce) public chainMinLimitedNonce;

    /* ========== VIEW FUNCTIONS ========== */

    /// @notice Returns whether the total amount, including the given token amount, will exceed the totalLimit.
    /// @dev The function will calculate the given token amount in USD.
    /// @param chainID The ID of the chain to check limit for.
    /// @param tokenID The ID of the token.
    /// @param amount The amount of the token.
    /// @return boolean indicating whether the total amount will exceed the limit.
    function willAmountExceedLimit(uint8 chainID, uint64 nonce, uint8 tokenID, uint256 amount)
        external
        view
        override
        returns (bool)
    {
        if (nonce < chainMinLimitedNonce[chainID]) return false;
        uint256 windowAmount = calculateWindowAmount(chainID);
        uint256 USDAmount = calculateAmountInUSD(tokenID, amount);
        return windowAmount + USDAmount > chainLimits[chainID];
    }

    /* ========== EXTERNAL FUNCTIONS ========== */

    /// @notice Updates the total limit with the provided message if the provided signatures are valid.
    /// @param signatures array of signatures to validate the message.
    /// @param message The BridgeUtils containing the update limit payload.
    function updateMinLimitNonceWithSignatures(
        bytes[] memory signatures,
        BridgeUtils.Message memory message
    )
        external
        nonReentrant
        verifyMessageAndSignatures(message, signatures, BridgeUtils.UPDATE_BRIDGE_LIMIT)
    {
        // decode the update limit payload
        (uint8 sourceChainID, uint64 newNonce) =
            BridgeUtilsV2.decodeUpdateMinLimitNoncePayload(message.payload);

        require(
            committee.config().isChainSupported(sourceChainID),
            "BridgeLimiter: Source chain not supported"
        );

        // update the chain limit
        chainMinLimitedNonce[sourceChainID] = newNonce;

        emit MinLimitNonceUpdated(sourceChainID, newNonce);
    }
}
