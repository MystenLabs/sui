// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title BridgeUtils
/// @notice This library defines the message format and constants for the Sui native bridge. It also
/// provides functions to encode and decode bridge messages and their payloads.
/// @dev This library only utilizes internal functions to enable upgradeability via the OpenZeppelin
/// UUPS proxy pattern (external libraries are not supported).
library BridgeUtilsV2 {
    /* ========== CONSTANTS ========== */

    // message Ids
    uint8 public constant UPDATE_MAX_SKIP_LIMITER_NONCE = 8;

    /* ========== INTERNAL FUNCTIONS ========== */

    /// @notice Decodes an update max skip limiter nonce payload from bytes to a chain ID and a new nonce.
    /// @dev The function will revert if the payload length is invalid.
    ///     Update limit payload is 8 bytes.
    ///     bytes 0-7    : new nonce
    /// @param _payload The payload to be decoded.
    /// @return newNonce the new nonce of the sending chain ID.
    function decodeUpdateMaxSkipLimiterNoncePayload(bytes memory _payload)
        internal
        pure
        returns (uint64 newNonce)
    {
        require(_payload.length == 8, "BridgeUtils: Invalid payload length");

        // Extracts the uint64 value by loading 32 bytes starting just after the first byte.
        // Position uint64 to the least significant bits by shifting it 192 bits to the right.
        assembly {
            newNonce := shr(192, mload(add(add(_payload, 0x20), 1)))
        }
    }
}
