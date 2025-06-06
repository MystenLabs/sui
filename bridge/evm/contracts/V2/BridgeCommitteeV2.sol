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

    mapping(uint256 epoch => mapping(address committeeMember => uint16 stakeAmount)) public
        committeeStakeAtEpoch;

    mapping(uint8 epoch => uint16 totalStake) public totalStake;

    uint8 public committeeEpoch;

    /* ========== INITIALIZERS ========== */

    /// @notice Initializes the contract with the provided parameters.
    /// @dev should be called directly after deployment (see OpenZeppelin upgradeable standards).
    /// the provided arrays must have the same length and the total stake provided must be greater than,
    /// or equal to the provided minimum stake required.
    /// @param committee addresses of the committee members.
    /// @param stake amounts of the committee members.
    /// @param minStakeRequired minimum stake required for the committee.
    function initialize(
        address[] memory committee,
        uint16[] memory stake,
        uint8 epoch,
        uint16 minStakeRequired
    ) external initializer {
        __CommitteeUpgradeable_init(address(this));
        __UUPSUpgradeable_init();

        uint256 _committeeLength = committee.length;

        require(_committeeLength < 256, "BridgeCommittee: Committee length must be less than 256");

        require(
            _committeeLength == stake.length,
            "BridgeCommittee: Committee and stake arrays must be of the same length"
        );

        uint16 _totalStake;
        for (uint16 i; i < _committeeLength; i++) {
            require(
                committeeStakeAtEpoch[epoch][committee[i]] == 0,
                "BridgeCommittee: Duplicate committee member"
            );
            committeeStakeAtEpoch[epoch][committee[i]] = stake[i];
            committeeIndex[committee[i]] = uint8(i);
            _totalStake += stake[i];
        }

        require(
            _totalStake >= minStakeRequired, "BridgeCommittee: total stake is less than minimum"
        ); // 10000 == 100%

        totalStake[epoch] = _totalStake;

        committeeEpoch = epoch;
    }

    /* ========== EXTERNAL FUNCTIONS ========== */

    /// @notice Verifies the provided signatures for the given message by aggregating and validating the
    /// stake of each signer against the required stake of the given message type.
    /// @dev The function will revert if the total stake of the signers is less than the required stake.
    /// @param epoch The epoch of the committee to verify signatures against.
    /// @param signatures The array of signatures to be verified.
    /// @param message The `BridgeUtils.Message` to be verified.
    function verifySignaturesV2(
        uint8 epoch,
        bytes[] memory signatures,
        BridgeUtils.Message memory message
    ) external view override {
        uint32 requiredStakePercent = BridgeUtils.requiredStake(message);

        uint16 stake;
        address signer;
        uint256 bitmap;

        // Check validity of each signature and aggregate the approval stake
        for (uint16 i; i < signatures.length; i++) {
            bytes memory signature = signatures[i];
            // recover the signer from the signature
            (bytes32 r, bytes32 s, uint8 v) = splitSignature(signature);

            (signer,,) = ECDSA.tryRecover(BridgeUtils.computeHash(message), v, r, s);

            require(!blocklist[signer], "BridgeCommittee: Signer is blocklisted");
            require(
                committeeStakeAtEpoch[epoch][signer] > 0, "BridgeCommittee: Signer has no stake"
            );

            uint8 index = committeeIndex[signer];
            uint256 mask = 1 << index;
            require(bitmap & mask == 0, "BridgeCommittee: Duplicate signature provided");
            bitmap |= mask;

            stake += committeeStakeAtEpoch[epoch][signer];
        }

        uint16 stakePercent = BridgeUtilsV2.convertToPercent(stake, totalStake[epoch]);

        require(stakePercent >= requiredStakePercent, "BridgeCommittee: Insufficient stake amount");
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

    // TODO: add new governance actions:

    /// @notice Updates the blocklist status of the provided addresses if provided signatures are valid.
    /// @param signatures The array of signatures to validate the message.
    /// @param message BridgeUtils containing the update blocklist payload.
    // function addToCommitteeWithSignatures(
    //     bytes[] memory signatures,
    //     BridgeUtils.Message memory message
    // )
    //     external
    //     nonReentrant
    //     verifyMessageAndSignatures(message, signatures, BridgeUtils.BLOCKLIST)
    // {
    //     uint256 _committeeLength = committee.length;

    //     require(_committeeLength < 256, "BridgeCommittee: Committee length must be less than 256");

    //     require(
    //         _committeeLength == stake.length,
    //         "BridgeCommittee: Committee and stake arrays must be of the same length"
    //     );

    //     uint16 stake;
    //     for (uint16 i; i < _committeeLength; i++) {
    //         require(
    //             committeeStakeAtEpoch[epoch][committee[i]] == 0,
    //             "BridgeCommittee: Duplicate committee member"
    //         );
    //         committeeStakeAtEpoch[epoch][committee[i]] = stake[i];
    //         committeeIndex[committee[i]] = uint8(i);
    //         stake += stake[i];
    //     }

    //     require(stake >= minStakeRequired, "BridgeCommittee: total stake is less than minimum"); // 10000 == 100%

    //     committeeEpoch = epoch;
    // }

    /* ========== INTERNAL FUNCTIONS ========== */

    /// @notice Updates the blocklist status of the provided addresses.
    /// @param _blocklist The addresses to update the blocklist status.
    /// @param isBlocklisted new blocklist status.
    // function _updateBlocklist(address[] memory _blocklist, bool isBlocklisted) private {
    //     // check original blocklist value of each validator
    //     for (uint16 i; i < _blocklist.length; i++) {
    //         blocklist[_blocklist[i]] = isBlocklisted;
    //     }
    // }
}
