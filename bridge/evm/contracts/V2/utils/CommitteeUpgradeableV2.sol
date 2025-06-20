// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "../../utils/CommitteeUpgradeable.sol";
import "./MessageVerifierV2.sol";

/// @title CommitteeUpgradeable
/// @notice This contract enables message signature verification using a BridgeCommittee contract,
/// in addition to providing an interface for upgradeability via signed message verification.
/// @dev The contract is intended to be inherited by contracts that require message verification and
/// upgradeability.
abstract contract CommitteeUpgradeableV2 is CommitteeUpgradeable, MessageVerifierV2 {
    /* ========== INITIALIZER ========== */

    /// @custom:oz-upgrades-unsafe-allow constructor
    constructor() {
        _disableInitializers();
    }

    function __CommitteeUpgradeableV2_init() internal onlyInitializing {
        __ReentrancyGuard_init();
        __MessageVerifierV2_init();
    }

    /* ========== EXTERNAL FUNCTIONS ========== */

    /// @notice Enables the upgrade of the inheriting contract by verifying the provided signatures.
    /// @dev The function will revert if the provided signatures or message is invalid.
    /// @param signatures The array of signatures to be verified.
    /// @param message The BridgeUtils to be verified.
    function upgradeWithSignatures(bytes[] memory signatures, BridgeUtilsV2.MessageV2 memory message)
        public
        virtual
        nonReentrant
        verifyMessageAndSignaturesV2(
            message,
            signatures,
            BridgeUtils.UPGRADE
        )
    {
        // decode the upgrade payload
        (address proxy, address implementation, bytes memory callData) =
            BridgeUtils.decodeUpgradePayload(message.payload);

        // verify proxy address
        require(proxy == address(this), "CommitteeUpgradeable: Invalid proxy address");

        // authorize upgrade
        _upgradeAuthorized = true;
        // upgrade contract
        upgradeToAndCall(implementation, callData);

        emit ContractUpgraded(message.nonce, proxy, implementation);
    }
}
