// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "../BridgeCommittee.sol";
import "./interfaces/IBridgeCommitteeV2.sol";
import "./utils/CommitteeUpgradeableV2.sol";
import "./utils/BridgeUtilsV2.sol";

/// @title BridgeCommittee
/// @notice This contract manages the committee members of the SuiBridge. The committee members are
/// responsible for signing messages used to update various bridge state including the committee itself.
/// The contract also provides functions to manage a blocklist of committee members whose signatures are invalid
/// once they are blocklisted.
contract BridgeCommitteeV2 is BridgeCommittee, IBridgeCommitteeV2, CommitteeUpgradeableV2 {
    /* ========== STATE VARIABLES ========== */

    // committeeID => committee member => stake amount
    mapping(uint16 committeeID => mapping(address committeeMember => uint16 stakeAmount)) public
        committeeStakeV2;

    // committeeID => totalStake
    mapping(uint16 committeeID => uint16 totalStake) public totalStake;

    uint8 public totalMembers;

    uint16 public committeeID;

    /* ========== INITIALIZERS ========== */

    /// @notice Initializes the contract with the provided parameters.
    /// @dev should be called directly after deployment (see OpenZeppelin upgradeable standards).
    /// the provided arrays must have the same length and the total stake provided must be greater than,
    /// or equal to the provided minimum stake required.
    /// @param committee addresses of the committee members.
    /// @param stake amounts of the committee members.
    function initialize(
        address[] memory committee,
        uint16[] memory stake,
        uint16 _committeeID
    ) external override initializer {
        __CommitteeUpgradeable_init(address(this));
        __UUPSUpgradeable_init();

        uint256 _committeeLength = committee.length;

        require(_committeeLength < 256, "BridgeCommitteeV2: Committee length must be less than 256");

        require(
            _committeeLength == stake.length,
            "BridgeCommitteeV2: Committee and stake arrays must be of the same length"
        );

        uint16 _totalStake;
        for (uint16 i; i < _committeeLength; i++) {
            require(
                committeeStakeV2[_committeeID][committee[i]] == 0,
                "BridgeCommitteeV2: Duplicate committee member"
            );
            committeeStakeV2[_committeeID][committee[i]] = stake[i];
            committeeIndex[committee[i]] = uint8(i);
            _totalStake += stake[i];
        }

        totalStake[_committeeID] = _totalStake;
        totalMembers = uint8(_committeeLength);
        committeeID = _committeeID;
    }

    /* ========== EXTERNAL FUNCTIONS ========== */

    /// @notice Verifies the provided signatures for the given message by aggregating and validating the
    /// stake of each signer against the required stake of the given message type.
    /// @dev The function will revert if the total stake of the signers is less than the required stake.
    /// @param signatures The array of signatures to be verified.
    /// @param message The `BridgeUtils.Message` to be verified.
    function verifySignaturesV2(
        bytes[] memory signatures,
        BridgeUtilsV2.MessageV2 memory message
    ) external view {
        uint32 requiredStakePercent = BridgeUtilsV2.requiredStake(message);

        uint16 stake;
        address signer;
        uint256 bitmap;

        // Check validity of each signature and aggregate the approval stake
        for (uint16 i; i < signatures.length; i++) {
            bytes memory signature = signatures[i];
            // recover the signer from the signature
            (bytes32 r, bytes32 s, uint8 v) = splitSignature(signature);

            (signer,,) = ECDSA.tryRecover(BridgeUtilsV2.computeHash(message), v, r, s);

            require(!blocklist[signer], "BridgeCommitteeV2: Signer is blocklisted");
            require(
                committeeStakeV2[message.committee][signer] > 0, "BridgeCommitteeV2: Signer has no stake"
            );

            uint8 index = committeeIndex[signer];
            uint256 mask = 1 << index;
            require(bitmap & mask == 0, "BridgeCommitteeV2: Duplicate signature provided");
            bitmap |= mask;

            stake += committeeStakeV2[message.committee][signer];
        }

        uint16 stakePercent = BridgeUtilsV2.convertToPercent(stake, totalStake[message.committee]);

        require(stakePercent >= requiredStakePercent, "BridgeCommitteeV2: Insufficient stake amount");
    }

    function addCommitteeMembersWithSignatures(
        bytes[] memory signatures,
        BridgeUtilsV2.MessageV2 memory message
    )
        external
        nonReentrant
        verifyMessageAndSignaturesV2(message, signatures, BridgeUtilsV2.ADD_COMMITTEE_MEMBERS)
    {
        // decode payload
        (address[] memory newMembers, uint16[] memory newStake) = 
            BridgeUtilsV2.decodeAddMembersPayload(message.payload);

        _addCommitteeMembers(message.committee, newMembers, newStake);

        // emit event
        emit CommitteeMembersAdded(newMembers, newStake); 
    }

    function syncCommitteeWithSignatures(
        bytes[] memory signatures,
        BridgeUtilsV2.MessageV2 memory message
    )
        external
        nonReentrant
        verifyMessageAndSignaturesV2(message, signatures, BridgeUtilsV2.SYNC_COMMITTEE)
    {
        // decode payload
        (address[] memory members, uint16[] memory stakes) = 
            BridgeUtilsV2.decodeSyncCommitteePayload(message.payload);

        _syncCommittee(message.committee, members, stakes);

        // emit event
        emit CommitteeMembersSynced(members, stakes, message.committee);
    }

    /* ========== INTERNAL FUNCTIONS ========== */

    function _addCommitteeMembers(uint8 _committeeID, address[] memory newMembers, uint16[] memory newStake) private {
        require(
            newMembers.length == newStake.length,
            "BridgeCommitteeV2: Members and stake arrays must be of the same length"
        );
        require(_committeeID > committeeID, "BridgeCommitteeV2: Committee ID must be greater than current");
        require(
            totalMembers + newMembers.length <= 256,
            "BridgeCommitteeV2: Total committee members must be less than 256"
        );
        require(totalStake[_committeeID] > 0, "BridgeCommitteeV2: Committee does not exist");

        uint16 totalNewStake;
        for (uint16 i; i < newMembers.length; i++) {
            committeeStakeV2[_committeeID][newMembers[i]] = newStake[i];
            committeeIndex[newMembers[i]] = uint8(totalMembers + i);
            totalNewStake += newStake[i];
        }
        
        totalMembers += uint8(newMembers.length);
        totalStake[_committeeID] = totalStake[committeeID] + totalNewStake;
    }

    function _syncCommittee(uint8 _committeeID, address[] memory members, uint16[] memory stakes) private {
        require(
            members.length == stakes.length,
            "BridgeCommitteeV2: Members and stake arrays must be of the same length"
        );
        require(_committeeID == committeeID, "BridgeCommitteeV2: Committee ID must match current");

        uint16 totalNewStake;
        for (uint16 i; i < members.length; i++) {
            committeeStakeV2[_committeeID][members[i]] = stakes[i];
            committeeIndex[members[i]] = uint8(i);
            totalNewStake += stakes[i];
        }

        totalStake[_committeeID] = totalNewStake;
        committeeID = _committeeID;
        totalMembers = uint8(members.length);
    }
}
