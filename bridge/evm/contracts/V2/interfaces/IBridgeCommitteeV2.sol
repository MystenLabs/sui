// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "../../interfaces/IBridgeCommittee.sol";
import "../utils/BridgeUtilsV2.sol";

/// @title IBridgeCommitteeV2
/// @notice Interface for the BridgeCommittee contract.
interface IBridgeCommitteeV2 is IBridgeCommittee {
    /// @notice Verifies the provided signatures for the given message by aggregating and validating the
    /// stake of each signer against the required stake of the given message type.
    /// @dev The function will revert if the total stake of the signers is less than the required stake.
    /// @param signatures The array of signatures to be verified.
    /// @param message The `BridgeUtils.Message` to be verified.
    function verifySignaturesV2(
        bytes[] memory signatures,
        BridgeUtilsV2.MessageV2 memory message
    ) external view;


    /// @dev (deprecated in favor of BlocklistUpdatedV2)
    event CommitteeMembersAdded(address[] newMembers, uint16[] stakeAmounts);

    event CommitteeMembersSynced(
        address[] newMembers,
        uint16[] stakeAmounts,
        uint8 committee
    );
}
