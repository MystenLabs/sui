// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "../BridgeConfig.sol";
import "./utils/CommitteeUpgradeableV2.sol";
import "./interfaces/IBridgeCommitteeV2.sol";

/// @title BridgeConfig
/// @notice This contract manages a registry of supported tokens and supported chain IDs for the SuiBridge.
/// It also provides functions to convert token amounts to Sui decimal adjusted amounts and vice versa.
contract BridgeConfigV2 is BridgeConfig, CommitteeUpgradeableV2 {
    /* ========== INITIALIZER ========== */

    function initialize() external initializer {
        committeeV2 = IBridgeCommitteeV2(address(committee));
    }

    /* ========== MUTATIVE FUNCTIONS ========== */

    /// @notice Updates the token price with the provided message if the provided signatures are valid.
    /// @param signatures array of signatures to validate the message.
    /// @param message BridgeMessage containing the update token price payload.
    function updateTokenPriceWithSignatures(
        bytes[] memory signatures,
        BridgeUtils.Message memory message
    )
        public
        override
        nonReentrant
        verifyMessageAndSignaturesV2(
            message,
            signatures,
            committeeV2.committeeEpoch(),
            BridgeUtils.UPDATE_TOKEN_PRICE
        )
    {
        updateTokenPriceWithSignatures(signatures, message);
    }

    function addTokensWithSignatures(bytes[] memory signatures, BridgeUtils.Message memory message)
        public
        override
        nonReentrant
        verifyMessageAndSignaturesV2(
            message,
            signatures,
            committeeV2.committeeEpoch(),
            BridgeUtils.ADD_EVM_TOKENS
        )
    {
        addTokensWithSignatures(signatures, message);
    }

    /// @notice Enables the upgrade of the inheriting contract by verifying the provided signatures.
    /// @dev The function will revert if the provided signatures or message is invalid.
    /// @param signatures The array of signatures to be verified.
    /// @param message The BridgeUtils to be verified.
    function upgradeWithSignatures(bytes[] memory signatures, BridgeUtils.Message memory message)
        public
        override(CommitteeUpgradeableV2, CommitteeUpgradeable)
        verifyMessageAndSignaturesV2(
            message,
            signatures,
            committeeV2.committeeEpoch(),
            BridgeUtils.ADD_EVM_TOKENS
        )
    {
        super.upgradeWithSignatures(signatures, message);
    }
}
