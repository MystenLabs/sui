// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "../utils/BridgeMessage.sol";

/// @title IBridgeCommittee
/// @notice Interface for the BridgeCommittee contract.
interface IBridgeCommittee {
    /// @notice Verifies the provided signatures for the given message by aggregating and validating the
    /// stake of each signer against the required stake of the given message type.
    /// @dev The function will revert if the total stake of the signers is less than the required stake.
    /// @param signatures The array of signatures to be verified.
    /// @param message The `BridgeMessage.Message` to be verified.
    function verifySignatures(bytes[] memory signatures, BridgeMessage.Message memory message)
        external
        view;

    /// @notice Returns the chain ID of the bridge.
    function chainID() external view returns (uint8);

    /* ========== EVENTS ========== */

    /// @notice Emitted when the blocklist is updated.
    /// @param newMembers The addresses of the new committee members.
    /// @param isBlocklisted A boolean indicating whether the committee members are blocklisted or not.
    event BlocklistUpdated(address[] newMembers, bool isBlocklisted);
}
