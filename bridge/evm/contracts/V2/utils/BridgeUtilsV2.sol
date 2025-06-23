// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "../../utils/BridgeUtils.sol";

library BridgeUtilsV2 {

    /* ========== CONSTANTS ========== */

    // message Ids
    uint8 public constant SYNC_COMMITTEE = 8;

    // Message type stake requirements
    uint32 public constant SYNC_COMMITTEE_STAKE_REQUIRED = 5001;

    /* ========== STRUCTS ========== */
    /// @dev A struct that represents a bridge message
    /// @param messageType The type of the message, such as token transfer, blocklist, etc.
    /// @param version The version of the message format
    /// @param nonce The nonce of the message, used to prevent replay attacks
    /// @param chainID The chain ID of the source chain (for token transfer messages this is the source chain)
    /// @param committee The committee number that processed the message
    /// @param timestamp The timestamp when the message was created, used to determine message maturity
    /// @param payload The payload of the message, which depends on the message type
    struct MessageV2 {
        uint8 messageType;
        uint8 version;
        uint64 nonce;
        uint8 chainID;
        uint8 committee;
        uint8 timestamp;
        bytes payload;
    }

    /* ========== CONSTANTS ========== */

    function convertToPercent(uint16 value, uint16 total) internal pure returns (uint16) {
        require(total > 0, "BridgeUtils: Total must be greater than zero");
        return (value * 100) / total;
    }

    function isMatureMessage(uint256 messageTimestamp, uint256 currentTimestamp)
        internal
        pure
        returns (bool)
    {
        // The message is considered mature if the timestamp is greater than or equal to the current block timestamp
        // minus 24 hours (24 * 3600 seconds).
        return currentTimestamp > messageTimestamp + 24 * 3600;
    }

        /// @notice returns the required stake for the provided message type.
    /// @dev The function will revert if the message type is invalid.
    /// @param _message The bridge message to be used to determine the required stake.
    /// @return The required stake for the provided message type.
    function requiredStake(MessageV2 memory _message) internal pure returns (uint32) {
        if (_message.messageType == BridgeUtils.TOKEN_TRANSFER) {
            return BridgeUtils.TRANSFER_STAKE_REQUIRED;
        } else if (_message.messageType == BridgeUtils.BLOCKLIST) {
            return BridgeUtils.BLOCKLIST_STAKE_REQUIRED;
        } else if (_message.messageType == BridgeUtils.EMERGENCY_OP) {
            bool isFreezing = BridgeUtils.decodeEmergencyOpPayload(_message.payload);
            if (isFreezing) return BridgeUtils.FREEZING_STAKE_REQUIRED;
            return BridgeUtils.UNFREEZING_STAKE_REQUIRED;
        } else if (_message.messageType == BridgeUtils.UPDATE_BRIDGE_LIMIT) {
            return BridgeUtils.BRIDGE_LIMIT_STAKE_REQUIRED;
        } else if (_message.messageType == BridgeUtils.UPDATE_TOKEN_PRICE) {
            return BridgeUtils.UPDATE_TOKEN_PRICE_STAKE_REQUIRED;
        } else if (_message.messageType == BridgeUtils.UPGRADE) {
            return BridgeUtils.UPGRADE_STAKE_REQUIRED;
        } else if (_message.messageType == BridgeUtils.ADD_EVM_TOKENS) {
            return BridgeUtils.ADD_EVM_TOKENS_STAKE_REQUIRED;
        } else if (_message.messageType == SYNC_COMMITTEE) {
            return SYNC_COMMITTEE_STAKE_REQUIRED;
        } else {
            revert("BridgeUtils: Invalid message type");
        }
    }

    /// @notice Computes the hash of a bridge message using keccak256.
    /// @param _message The bridge message to be hashed.
    /// @return The hash of the message.
    function computeHash(MessageV2 memory _message) internal pure returns (bytes32) {
        return keccak256(encodeMessage(_message));
    }

    /// @notice Encodes a bridge message into bytes, using abi.encodePacked to concatenate the message fields.
    /// @param message The bridge message to be encoded.
    /// @return The encoded message as bytes.
    function encodeMessage(MessageV2 memory message) internal pure returns (bytes memory) {
        bytes memory prefixTypeAndVersion =
            abi.encodePacked(BridgeUtils.MESSAGE_PREFIX, message.messageType, message.version);
        bytes memory nonce = abi.encodePacked(message.nonce);
        bytes memory chainID = abi.encodePacked(message.chainID);
        return bytes.concat(prefixTypeAndVersion, nonce, chainID, message.payload);
    }

    /// @notice Decodes an add members payload from bytes to an array of addresses 
    /// and an array of stake integers.
    /// @dev The function will revert if the payload length is invalid.
    ///     Add members payload is 2 bytes + 20 * n bytes + 2 * m bytes.
    ///     byte 0       : number of new addresses 
    ///     bytes 1-n    : addresses
    ///     byte n+1     : number of stake amounts
    ///     bytes n+2-m  : stake amounts
    /// @param _payload the payload to be decoded.
    /// @return members the array of decoded addresses
    /// @return stakeAmounts the array of decoded stake amounts
    function decodeAddMembersPayload(bytes memory _payload)
        internal
        pure
        returns (address[] memory members, uint16[] memory stakeAmounts)
    {
        uint8 membersLength = uint8(_payload[0]);
        members = new address[](membersLength);
        uint8 offset = 1; // Start after the first byte which is the length of the members array
        // require((_payload.length - offset) % 20 == 0, "BridgeUtils: Invalid payload length");
        for (uint8 i; i < membersLength; i++) {
            // Calculate the starting index for each address
            offset += i * 20;
            address member;
            // Extract each address
            assembly {
                member := mload(add(add(_payload, 20), offset))
            }
            // Store the extracted address
            members[i] = member;
        }

        uint8 stakeLength = uint8(_payload[offset++]);
        stakeAmounts = new uint16[](stakeLength);
        for (uint8 i; i < stakeLength; i++) {
            // Calculate the starting index for each stake amount
            offset += i * 2;
            uint16 stakeAmount;
            // Extract each stake amount
            assembly {
                stakeAmount := mload(add(add(_payload, 2), offset))
            }
            // Store the extracted stake amount
            stakeAmounts[i] = stakeAmount;
        }
    }
    
    /// @notice Decodes sync committee payload from bytes to an array of addresses 
    /// and an array of stake integers.
    /// @dev The function will revert if the payload length is invalid.
    ///     Sync committee payload is 2 bytes + 20 * n bytes + 2 * m bytes.
    ///     byte 0       : number of addresses
    ///     bytes 1-n    : addresses
    ///     byte n+1     : number of stake amounts
    ///     bytes n+2-m  : stake amounts
    /// @param _payload the payload to be decoded.
    /// @return members the array of decoded addresses
    /// @return stakeAmounts the array of decoded stake amounts
    function decodeSyncCommitteePayload(bytes memory _payload)
        internal
        pure
        returns (address[] memory members, uint16[] memory stakeAmounts)
    {
        uint8 membersLength = uint8(_payload[0]);
        members = new address[](membersLength);
        uint8 offset = 1; // Start after the first byte which is the length of the members array
        // require((_payload.length - offset) % 20 == 0, "BridgeUtils: Invalid payload length");
        for (uint8 i; i < membersLength; i++) {
            // Calculate the starting index for each address
            offset += i * 20;
            address member;
            // Extract each address
            assembly {
                member := mload(add(add(_payload, 20), offset))
            }
            // Store the extracted address
            members[i] = member;
        }

        uint8 stakeLength = uint8(_payload[offset++]);
        stakeAmounts = new uint16[](stakeLength);
        for (uint8 i; i < stakeLength; i++) {
            // Calculate the starting index for each stake amount
            offset += i * 2;
            uint16 stakeAmount;
            // Extract each stake amount
            assembly {
                stakeAmount := mload(add(add(_payload, 2), offset))
            }
            // Store the extracted stake amount
            stakeAmounts[i] = stakeAmount;
        }
    }

}
