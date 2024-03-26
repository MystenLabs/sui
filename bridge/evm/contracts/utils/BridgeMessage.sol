// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title BridgeMessage
/// @notice This library defines the message format and constants for the Sui native bridge. It also
/// provides functions to encode and decode bridge messages and their payloads.
/// @dev This library only utilizes internal functions to enable upgradeability via the OpenZeppelin
/// UUPS proxy pattern (external libraries are not supported).
library BridgeMessage {
    /* ========== STRUCTS ========== */

    /// @dev A struct that represents a bridge message
    /// @param messageType The type of the message, such as token transfer, blocklist, etc.
    /// @param version The version of the message format
    /// @param nonce The nonce of the message, used to prevent replay attacks
    /// @param chainID The chain ID of the source chain (for token transfer messages this is the source chain)
    /// @param payload The payload of the message, which depends on the message type
    struct Message {
        uint8 messageType;
        uint8 version;
        uint64 nonce;
        uint8 chainID;
        bytes payload;
    }

    /// @dev A struct that represents a token transfer payload
    /// @param senderAddressLength The length of the sender address in bytes
    /// @param senderAddress The address of the sender on the source chain
    /// @param targetChain The chain ID of the target chain
    /// @param recipientAddressLength The length of the target address in bytes
    /// @param recipientAddress The address of the recipient on the target chain
    /// @param tokenID The ID of the token to be transferred
    /// @param amount The amount of the token to be transferred
    struct TokenTransferPayload {
        uint8 senderAddressLength;
        bytes senderAddress;
        uint8 targetChain;
        uint8 recipientAddressLength;
        address recipientAddress;
        uint8 tokenID;
        uint64 amount;
    }

    /* ========== CONSTANTS ========== */

    // message Ids
    uint8 public constant TOKEN_TRANSFER = 0;
    uint8 public constant BLOCKLIST = 1;
    uint8 public constant EMERGENCY_OP = 2;
    uint8 public constant UPDATE_BRIDGE_LIMIT = 3;
    uint8 public constant UPDATE_TOKEN_PRICE = 4;
    uint8 public constant UPGRADE = 5;

    // Message type stake requirements
    uint32 public constant TRANSFER_STAKE_REQUIRED = 3334;
    uint32 public constant FREEZING_STAKE_REQUIRED = 450;
    uint32 public constant UNFREEZING_STAKE_REQUIRED = 5001;
    uint32 public constant UPGRADE_STAKE_REQUIRED = 5001;
    uint16 public constant BLOCKLIST_STAKE_REQUIRED = 5001;
    uint32 public constant BRIDGE_LIMIT_STAKE_REQUIRED = 5001;
    uint32 public constant TOKEN_PRICE_STAKE_REQUIRED = 5001;

    // token Ids
    uint8 public constant SUI = 0;
    uint8 public constant BTC = 1;
    uint8 public constant ETH = 2;
    uint8 public constant USDC = 3;
    uint8 public constant USDT = 4;

    string public constant MESSAGE_PREFIX = "SUI_BRIDGE_MESSAGE";

    /* ========== INTERNAL FUNCTIONS ========== */

    /// @notice Encodes a bridge message into bytes, using abi.encodePacked to concatenate the message fields.
    /// @param message The bridge message to be encoded.
    /// @return The encoded message as bytes.
    function encodeMessage(Message memory message) internal pure returns (bytes memory) {
        bytes memory prefixTypeAndVersion =
            abi.encodePacked(MESSAGE_PREFIX, message.messageType, message.version);
        bytes memory nonce = abi.encodePacked(message.nonce);
        bytes memory chainID = abi.encodePacked(message.chainID);
        return bytes.concat(prefixTypeAndVersion, nonce, chainID, message.payload);
    }

    /// @notice Computes the hash of a bridge message using keccak256.
    /// @param _message The bridge message to be hashed.
    /// @return The hash of the message.
    function computeHash(Message memory _message) internal pure returns (bytes32) {
        return keccak256(encodeMessage(_message));
    }

    /// @notice returns the required stake for the provided message type.
    /// @dev The function will revert if the message type is invalid.
    /// @param _message The bridge message to be used to determine the required stake.
    /// @return The required stake for the provided message type.
    function requiredStake(Message memory _message) internal pure returns (uint32) {
        if (_message.messageType == TOKEN_TRANSFER) {
            return TRANSFER_STAKE_REQUIRED;
        } else if (_message.messageType == BLOCKLIST) {
            return BLOCKLIST_STAKE_REQUIRED;
        } else if (_message.messageType == EMERGENCY_OP) {
            bool isFreezing = decodeEmergencyOpPayload(_message.payload);
            if (isFreezing) return FREEZING_STAKE_REQUIRED;
            return UNFREEZING_STAKE_REQUIRED;
        } else if (_message.messageType == UPDATE_BRIDGE_LIMIT) {
            return BRIDGE_LIMIT_STAKE_REQUIRED;
        } else if (_message.messageType == UPDATE_TOKEN_PRICE) {
            return TOKEN_PRICE_STAKE_REQUIRED;
        } else if (_message.messageType == UPGRADE) {
            return UPGRADE_STAKE_REQUIRED;
        } else {
            revert("BridgeMessage: Invalid message type");
        }
    }

    /// @notice Decodes a token transfer payload from bytes to a TokenTransferPayload struct.
    /// @dev The function will revert if the payload length is invalid.
    ///     TokenTransfer payload is 64 bytes.
    ///     byte 0       : sender address length
    ///     bytes 1-32   : sender address (as we only support Sui now, it has to be 32 bytes long)
    ///     bytes 33     : target chain id
    ///     byte 34      : target address length
    ///     bytes 35-54  : target address
    ///     byte 55      : token id
    ///     bytes 56-63  : amount
    /// @param _payload The payload to be decoded.
    /// @return The decoded token transfer payload as a TokenTransferPayload struct.
    function decodeTokenTransferPayload(bytes memory _payload)
        internal
        pure
        returns (BridgeMessage.TokenTransferPayload memory)
    {
        require(_payload.length == 64, "BridgeMessage: TokenTransferPayload must be 64 bytes");

        uint8 senderAddressLength = uint8(_payload[0]);

        require(
            senderAddressLength == 32,
            "BridgeMessage: Invalid sender address length, Sui address must be 32 bytes"
        );

        // used to offset already read bytes
        uint8 offset = 1;

        // extract sender address from payload bytes 1-32
        bytes memory senderAddress = new bytes(senderAddressLength);
        for (uint256 i; i < senderAddressLength; i++) {
            senderAddress[i] = _payload[i + offset];
        }

        // move offset past the sender address length
        offset += senderAddressLength;

        // target chain is a single byte
        uint8 targetChain = uint8(_payload[offset++]);

        // target address length is a single byte
        uint8 recipientAddressLength = uint8(_payload[offset++]);
        require(
            recipientAddressLength == 20,
            "BridgeMessage: Invalid target address length, EVM address must be 20 bytes"
        );

        // extract target address from payload (35-54)
        address recipientAddress;

        // why `add(recipientAddressLength, offset)`?
        // At this point, offset = 35, recipientAddressLength = 20. `mload(add(payload, 55))`
        // reads the next 32 bytes from bytes 23 in paylod, because the first 32 bytes
        // of payload stores its length. So in reality, bytes 23 - 54 is loaded. During
        // casting to address (20 bytes), the least sigificiant bytes are retained, namely
        // `recipientAddress` is bytes 35-54
        assembly {
            recipientAddress := mload(add(_payload, add(recipientAddressLength, offset)))
        }

        // move offset past the target address length
        offset += recipientAddressLength;

        // token id is a single byte
        uint8 tokenID = uint8(_payload[offset++]);

        // extract amount from payload
        uint64 amount;
        uint8 amountLength = 8; // uint64 = 8 bits

        // Why `add(amountLength, offset)`?
        // At this point, offset = 56, amountLength = 8. `mload(add(payload, 64))`
        // reads the next 32 bytes from bytes 32 in paylod, because the first 32 bytes
        // of payload stores its length. So in reality, bytes 32 - 63 is loaded. During
        // casting to uint64 (8 bytes), the least sigificiant bytes are retained, namely
        // `recipientAddress` is bytes 56-63
        assembly {
            amount := mload(add(_payload, add(amountLength, offset)))
        }

        return TokenTransferPayload(
            senderAddressLength,
            senderAddress,
            targetChain,
            recipientAddressLength,
            recipientAddress,
            tokenID,
            amount
        );
    }

    /// @notice Decodes a blocklist payload from bytes to a boolean and an array of addresses.
    /// @dev The function will revert if the payload length is invalid.
    ///     Blocklist payload is 2 + 20 * n bytes.
    ///     byte 0       : blocklist type (0 = blocklist, 1 = unblocklist)
    ///     byte 1       : number of addresses in the blocklist
    ///     bytes 2-n    : addresses
    /// @param _payload The payload to be decoded.
    /// @return blocklisting status and the array of addresses to be blocklisted/unblocklisted.
    function decodeBlocklistPayload(bytes memory _payload)
        internal
        pure
        returns (bool, address[] memory)
    {
        uint8 blocklistType = uint8(_payload[0]);
        uint8 membersLength = uint8(_payload[1]);
        address[] memory members = new address[](membersLength);
        uint8 offset = 2;
        require((_payload.length - offset) % 20 == 0, "BridgeMessage: Invalid payload length");
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
        // blocklistType: 0 = blocklist, 1 = unblocklist
        bool blocklisted = (blocklistType == 0);
        return (blocklisted, members);
    }

    /// @notice Decodes an emergency operation payload from bytes to a boolean.
    /// @dev The function will revert if the payload length is invalid.
    ///     Emergency operation payload is a single byte.
    ///     byte 0       : operation type (0 = freezing, 1 = unfreezing)
    /// @param _payload The payload to be decoded.
    /// @return The emergency operation type.
    function decodeEmergencyOpPayload(bytes memory _payload) internal pure returns (bool) {
        require(_payload.length == 1, "BridgeMessage: Invalid payload length");
        uint8 emergencyOpCode = uint8(_payload[0]);
        require(emergencyOpCode <= 1, "BridgeMessage: Invalid op code");
        return emergencyOpCode == 0;
    }

    /// @notice Decodes an update limit payload from bytes to a chain ID and a new limit.
    /// @dev The function will revert if the payload length is invalid.
    ///     Update limit payload is 9 bytes.
    ///     byte 0       : chain ID
    ///     bytes 1-8    : new limit
    /// @param _payload The payload to be decoded.
    /// @return senderChainID the sending chain ID to update the limit of.
    /// @return newLimit the new limit of the sending chain ID.
    function decodeUpdateLimitPayload(bytes memory _payload)
        internal
        pure
        returns (uint8 senderChainID, uint64 newLimit)
    {
        require(_payload.length == 9, "BridgeMessage: Invalid payload length");
        senderChainID = uint8(_payload[0]);

        // Extracts the uint64 value by loading 32 bytes starting just after the first byte.
        // Position uint64 to the least significant bits by shifting it 192 bits to the right.
        assembly {
            newLimit := shr(192, mload(add(add(_payload, 0x20), 1)))
        }
    }

    /// @notice Decodes an update token price payload from bytes to a token ID and a new price.
    /// @dev The function will revert if the payload length is invalid.
    ///     Update token price payload is 9 bytes.
    ///     byte 0       : token ID
    ///     bytes 1-8    : new price
    /// @param _payload The payload to be decoded.
    /// @return tokenID the token ID to update the price of.
    /// @return tokenPrice the new price of the token.
    function decodeUpdateTokenPricePayload(bytes memory _payload)
        internal
        pure
        returns (uint8 tokenID, uint64 tokenPrice)
    {
        require(_payload.length == 9, "BridgeMessage: Invalid payload length");
        tokenID = uint8(_payload[0]);

        // Extracts the uint64 value by loading 32 bytes starting just after the first byte.
        // Position uint64 to the least significant bits by shifting it 192 bits to the right.
        assembly {
            tokenPrice := shr(192, mload(add(add(_payload, 0x20), 1)))
        }
    }

    /// @notice Decodes an upgrade payload from bytes to a proxy address, an implementation address,
    /// and call data.
    /// @dev The function will revert if the payload length is invalid. The payload is expected to be
    /// abi encoded.
    /// @param _payload The payload to be decoded.
    /// @return proxy the address of the proxy to be upgraded.
    /// @return implementation the address of the new implementation contract.
    /// @return callData the call data to be used in the upgrade.
    function decodeUpgradePayload(bytes memory _payload)
        internal
        pure
        returns (address, address, bytes memory)
    {
        (address proxy, address implementation, bytes memory callData) =
            abi.decode(_payload, (address, address, bytes));
        return (proxy, implementation, callData);
    }
}
