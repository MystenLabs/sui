// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "@openzeppelin/contracts-upgradeable/proxy/utils/Initializable.sol";
import "@openzeppelin/contracts-upgradeable/utils/ReentrancyGuardUpgradeable.sol";
import "@openzeppelin/contracts-upgradeable/proxy/utils/UUPSUpgradeable.sol";
import "../interfaces/IBridgeCommittee.sol";
import "./MessageVerifier.sol";

/// @title CommitteeUpgradeable
/// @notice This contract enables message signature verification using a BridgeCommittee contract,
/// in addition to providing an interface for upgradeability via signed message verification.
/// @dev The contract is intended to be inherited by contracts that require message verification and
/// upgradeability.
abstract contract CommitteeUpgradeable is
    UUPSUpgradeable,
    MessageVerifier,
    ReentrancyGuardUpgradeable
{
    /* ========== STATE VARIABLES ========== */

    bool private _upgradeAuthorized;
    // upgradeablity storage gap
    uint256[50] private __gap;

    /* ========== INITIALIZER ========== */

    /// @custom:oz-upgrades-unsafe-allow constructor
    constructor() {
        _disableInitializers();
    }

    function __CommitteeUpgradeable_init(address _committee) internal onlyInitializing {
        __ReentrancyGuard_init();
        __MessageVerifier_init(_committee);
        committee = IBridgeCommittee(_committee);
    }

    /* ========== EXTERNAL FUNCTIONS ========== */

    /// @notice Enables the upgrade of the inheriting contract by verifying the provided signatures.
    /// @dev The function will revert if the provided signatures or message is invalid.
    /// @param signatures The array of signatures to be verified.
    /// @param message The BridgeUtils to be verified.
    function upgradeWithSignatures(bytes[] memory signatures, BridgeUtils.Message memory message)
        external
        nonReentrant
        verifyMessageAndSignatures(message, signatures, BridgeUtils.UPGRADE)
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

    /* ========== INTERNAL FUNCTIONS ========== */

    /// @notice Authorizes the upgrade of the inheriting contract.
    /// @dev The _upgradeAuthorized state variable can only be set with the upgradeWithSignatures
    /// function, meaning that the upgrade can only be authorized by the committee.
    function _authorizeUpgrade(address) internal override {
        require(_upgradeAuthorized, "CommitteeUpgradeable: Unauthorized upgrade");
        _upgradeAuthorized = false;
    }

    /// @notice Event emitted when the contract is upgraded
    /// @param nonce The nonce of the upgrade message.
    /// @param proxy The address of the proxy contract.
    /// @param implementation The address of the new implementation.
    event ContractUpgraded(uint256 nonce, address proxy, address implementation);
}
