// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

library BridgeUtilsV2 {
    /* ========== STRUCTS ========== */
    /// @dev A struct that represents a token transfer payload
    /// @param senderAddressLength The length of the sender address in bytes
    /// @param senderAddress The address of the sender on the source chain
    /// @param targetChain The chain ID of the target chain
    /// @param recipientAddressLength The length of the target address in bytes
    /// @param recipientAddress The address of the recipient on the target chain
    /// @param tokenID The ID of the token to be transferred
    /// @param amount The amount of the token to be transferred
    /// @param timestamp The timestamp of the message creation
    struct TokenTransferPayloadV2 {
        uint8 senderAddressLength;
        bytes senderAddress;
        uint8 targetChain;
        uint8 recipientAddressLength;
        address recipientAddress;
        uint8 tokenID;
        uint64 amount;
        uint256 timestampMs;
    }

    /* ========== CONSTANTS ========== */

    function isMatureMessage(uint256 messageTimestamp, uint256 currentTimestamp)
        internal
        pure
        returns (bool)
    {
        // The message is considered mature if the timestamp is greater than or equal to the current block timestamp
        // minus 48 hours (48 * 3600 seconds).
        return currentTimestamp > messageTimestamp + 48 * 3600;
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
    ///     bytes 64-71  : message timestamp
    /// @param _payload The payload to be decoded.
    /// @return The decoded token transfer payload as a TokenTransferPayload struct.
    function decodeTokenTransferPayloadV2(bytes memory _payload)
        internal
        pure
        returns (TokenTransferPayloadV2 memory)
    {
        require(_payload.length == 72, "BridgeUtils: TokenTransferPayload must be 72 bytes");

        uint8 senderAddressLength = uint8(_payload[0]);

        require(
            senderAddressLength == 32,
            "BridgeUtils: Invalid sender address length, Sui address must be 32 bytes"
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
            "BridgeUtils: Invalid target address length, EVM address must be 20 bytes"
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

        // Extract timestamp from payload bytes 64-71
        // Similar to amount extraction: offset 72 = 32 (length prefix) + 40 (data position)
        // reads bytes 40-71, casting to uint64 keeps low 8 bytes (64-71)
        uint64 timestamp64;
        assembly {
            timestamp64 := mload(add(_payload, 72))
        }
        uint256 message_timestamp = uint256(timestamp64);

        return TokenTransferPayloadV2(
            senderAddressLength,
            senderAddress,
            targetChain,
            recipientAddressLength,
            recipientAddress,
            tokenID,
            amount,
            message_timestamp
        );
    }
}
