// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "@openzeppelin/contracts/utils/cryptography/ECDSA.sol";
import "./interfaces/IBridgeCommittee.sol";
import "./utils/CommitteeUpgradeable.sol";

/// @title BridgeCommittee
/// @notice This contract manages the committee members of the SuiBridge. The committee members are
/// responsible for signing messages used to update various bridge state including the committee itself.
/// The contract also provides functions to manage a blocklist of committee members whose signatures are invalid
/// once they are blocklisted.
contract BridgeCommittee is IBridgeCommittee, CommitteeUpgradeable {
    /* ========== STATE VARIABLES ========== */

    mapping(address committeeMember => uint16 stakeAmount) public committeeStake;
    mapping(address committeeMember => uint8 index) public committeeIndex;
    mapping(address committeeMember => bool isBlocklisted) public blocklist;
    IBridgeConfig public config;

    /* ========== INITIALIZERS ========== */

    /// @notice Initializes the contract with the provided parameters.
    /// @dev should be called directly after deployment (see OpenZeppelin upgradeable standards).
    /// the provided arrays must have the same length and the total stake provided must be greater than,
    /// or equal to the provided minimum stake required.
    /// @param committee addresses of the committee members.
    /// @param stake amounts of the committee members.
    /// @param minStakeRequired minimum stake required for the committee.
    function initialize(address[] memory committee, uint16[] memory stake, uint16 minStakeRequired)
        external
        initializer
    {
        __CommitteeUpgradeable_init(address(this));
        __UUPSUpgradeable_init();

        uint256 _committeeLength = committee.length;

        require(_committeeLength < 256, "BridgeCommittee: Committee length must be less than 256");

        require(
            _committeeLength == stake.length,
            "BridgeCommittee: Committee and stake arrays must be of the same length"
        );

        uint16 totalStake;
        for (uint16 i; i < _committeeLength; i++) {
            require(
                committeeStake[committee[i]] == 0, "BridgeCommittee: Duplicate committee member"
            );
            committeeStake[committee[i]] = stake[i];
            committeeIndex[committee[i]] = uint8(i);
            totalStake += stake[i];
        }

        require(totalStake >= minStakeRequired, "BridgeCommittee: total stake is less than minimum"); // 10000 == 100%
    }

    /// @notice Initializes the contract with the provided parameters.
    /// @dev This function should be called directly after config deployment. The config contract address
    /// provided should be verified before bridging any assets.
    /// @param _config The address of the BridgeConfig contract.
    function initializeConfig(address _config) external {
        require(address(config) == address(0), "BridgeCommittee: Config already initialized");
        config = IBridgeConfig(_config);
    }

    /* ========== EXTERNAL FUNCTIONS ========== */

    /// @notice Verifies the provided signatures for the given message by aggregating and validating the
    /// stake of each signer against the required stake of the given message type.
    /// @dev The function will revert if the total stake of the signers is less than the required stake.
    /// @param signatures The array of signatures to be verified.
    /// @param message The `BridgeUtils.Message` to be verified.
    function verifySignatures(bytes[] memory signatures, BridgeUtils.Message memory message)
        external
        view
        override
    {
        uint32 requiredStake = BridgeUtils.requiredStake(message);

        uint16 approvalStake;
        address signer;
        uint256 bitmap;

        // Check validity of each signature and aggregate the approval stake
        for (uint16 i; i < signatures.length; i++) {
            bytes memory signature = signatures[i];
            // recover the signer from the signature
            (bytes32 r, bytes32 s, uint8 v) = splitSignature(signature);

            (signer,,) = ECDSA.tryRecover(BridgeUtils.computeHash(message), v, r, s);

            require(!blocklist[signer], "BridgeCommittee: Signer is blocklisted");
            require(committeeStake[signer] > 0, "BridgeCommittee: Signer has no stake");

            uint8 index = committeeIndex[signer];
            uint256 mask = 1 << index;
            require(bitmap & mask == 0, "BridgeCommittee: Duplicate signature provided");
            bitmap |= mask;

            approvalStake += committeeStake[signer];
        }

        require(approvalStake >= requiredStake, "BridgeCommittee: Insufficient stake amount");
    }

    /// @notice Updates the blocklist status of the provided addresses if provided signatures are valid.
    /// @param signatures The array of signatures to validate the message.
    /// @param message BridgeUtils containing the update blocklist payload.
    function updateBlocklistWithSignatures(
        bytes[] memory signatures,
        BridgeUtils.Message memory message
    )
        external
        nonReentrant
        verifyMessageAndSignatures(message, signatures, BridgeUtils.BLOCKLIST)
    {
        // decode the blocklist payload
        (bool isBlocklisted, address[] memory _blocklist) =
            BridgeUtils.decodeBlocklistPayload(message.payload);

        // update the blocklist
        _updateBlocklist(_blocklist, isBlocklisted);

        emit BlocklistUpdatedV2(message.nonce, _blocklist, isBlocklisted);
    }

    /* ========== INTERNAL FUNCTIONS ========== */

    /// @notice Updates the blocklist status of the provided addresses.
    /// @param _blocklist The addresses to update the blocklist status.
    /// @param isBlocklisted new blocklist status.
    function _updateBlocklist(address[] memory _blocklist, bool isBlocklisted) private {
        // check original blocklist value of each validator
        for (uint16 i; i < _blocklist.length; i++) {
            blocklist[_blocklist[i]] = isBlocklisted;
        }
    }

    /// @notice Splits the provided signature into its r, s, and v components.
    /// @param sig The signature to be split.
    /// @return r The r component of the signature.
    /// @return s The s component of the signature.
    /// @return v The v component of the signature.
    function splitSignature(bytes memory sig)
        internal
        pure
        returns (bytes32 r, bytes32 s, uint8 v)
    {
        require(sig.length == 65, "BridgeCommittee: Invalid signature length");
        // ecrecover takes the signature parameters, and the only way to get them
        // currently is to use assembly.
        /// @solidity memory-safe-assembly
        assembly {
            r := mload(add(sig, 32))
            s := mload(add(sig, 64))
            v := byte(0, mload(add(sig, 96)))
        }

        //adjust for ethereum signature verification
        if (v < 27) v += 27;
    }
}
