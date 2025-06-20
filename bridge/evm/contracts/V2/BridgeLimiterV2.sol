// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "../BridgeLimiter.sol";
import "./utils/CommitteeUpgradeableV2.sol";

/// @title BridgeLimiter
/// @notice A contract that limits the amount of tokens that can be bridged from a given chain within
/// a rolling 24-hour window. This is accomplished by storing the amount bridged from a given chain in USD
/// within a given hourly timestamp. It also provides functions to update the token prices and the total
/// limit of the given chainID measured in USD with 8 decimal precision.
/// The contract is intended to be used and owned by the SuiBridge contract.
contract BridgeLimiterV2 is BridgeLimiter, CommitteeUpgradeableV2 {
    /* ========== INITIALIZER ========== */

    function initialize() external initializer {
        committeeV2 = IBridgeCommitteeV2(address(committee));
    }

    /// @notice Updates the total limit with the provided message if the provided signatures are valid.
    /// @param signatures array of signatures to validate the message.
    /// @param message The BridgeUtils containing the update limit payload.
    function updateLimitWithSignatures(
        bytes[] memory signatures,
        BridgeUtils.Message memory message
    )
        public
        override
        nonReentrant
        verifyMessageAndSignatures(
            message,
            signatures,
            BridgeUtils.UPDATE_BRIDGE_LIMIT
        )
    {
        // decode the update limit payload
        (uint8 sourceChainID, uint64 newLimit) =
            BridgeUtils.decodeUpdateLimitPayload(message.payload);

        require(
            committee.config().isChainSupported(sourceChainID),
            "BridgeLimiter: Source chain not supported"
        );

        // update the chain limit
        chainLimits[sourceChainID] = newLimit;

        emit LimitUpdatedV2(message.nonce, sourceChainID, newLimit);
    }
}
