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
    /* ========== STRUCTS ========== */
    struct CommitteeMember {
        uint8 index;
        uint16 stake;
    }

    /* ========== STATE VARIABLES ========== */

    // committeeID => committee member => stake amount
    mapping(uint16 committeeID => mapping(address committeeMember => CommitteeMember member)) public
        committeeMembers;


    // committeeID => totalStake
    mapping(uint16 committeeID => uint16 totalStake) public totalStake;

    mapping(uint16 committeeID => uint8 totalMembers) public totalMembers;

    uint16 public committeeID;

    /* ========== INITIALIZERS ========== */

    /// @notice Initializes the contract with the provided parameters.
    /// @dev should be called directly after deployment (see OpenZeppelin upgradeable standards).
    /// the provided arrays must have the same length and the total stake provided must be greater than,
    /// or equal to the provided minimum stake required.
    /// @param committee addresses of the committee members.
    /// @param stake amounts of the committee members.
    function initialize_V2(
        address[] memory committee,
        uint16[] memory stake,
        uint16 _committeeID
    ) external initializer {
        __CommitteeUpgradeable_init(address(this));
        __UUPSUpgradeable_init();

        uint256 _committeeLength = committee.length;

        require(_committeeLength < 256, "BridgeCommitteeV2: Committee length must be less than 256");

        require(
            _committeeLength == stake.length,
            "BridgeCommitteeV2: Committee and stake arrays must be of the same length"
        );

        _syncCommittee(_committeeID, committee, stake);


        uint16 _totalStake;
        for (uint16 i; i < _committeeLength; i++) {
            committeeMembers[_committeeID][committee[i]] = CommitteeMember({
                stake: stake[i],
                index: uint8(i)
            });
            _totalStake += stake[i];
        }

        totalStake[_committeeID] = _totalStake;
        totalMembers[_committeeID] = uint8(_committeeLength);
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

            CommitteeMember memory member = committeeMembers[message.committee][signer];

            require(
                member.stake > 0, "BridgeCommitteeV2: Signer has no stake"
            );

            uint8 index = member.index;
            uint256 mask = 1 << index;
            require(bitmap & mask == 0, "BridgeCommitteeV2: Duplicate signature provided");
            bitmap |= mask;

            stake += member.stake;
        }

        uint16 stakePercent = BridgeUtilsV2.convertToPercent(stake, totalStake[message.committee]);

        require(stakePercent >= requiredStakePercent, "BridgeCommitteeV2: Insufficient stake amount");
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

    function _syncCommittee(uint16 _committeeID, address[] memory members, uint16[] memory stakes) private {
        require(
            members.length == stakes.length,
            "BridgeCommitteeV2: Members and stake arrays must be of the same length"
        );
        require(_committeeID > committeeID, "BridgeCommitteeV2: Committee ID must be greater than current ID");

        uint16 totalNewStake;
        for (uint16 i; i < members.length; i++) {
            CommitteeMember storage member = committeeMembers[_committeeID][members[i]];
            require(
                member.stake == 0,
                "BridgeCommitteeV2: Duplicate committee member"
            );
            member.stake = stakes[i];
            member.index = uint8(i);
            totalNewStake += stakes[i];
        }

        totalStake[_committeeID] = totalNewStake;
        totalMembers[_committeeID] = uint8(members.length);
        committeeID = _committeeID;
    }
}
