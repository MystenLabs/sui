// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title BridgeUtils
/// @notice This library defines the message format and constants for the Sui native bridge. It also
/// provides functions to encode and decode bridge messages and their payloads.
/// @dev This library only utilizes internal functions to enable upgradeability via the OpenZeppelin
/// UUPS proxy pattern (external libraries are not supported).
library BridgeUtilsV2 {
    /* ========== INTERNAL FUNCTIONS ========== */

    /// @notice Decodes an update limit payload from bytes to a chain ID and a new limit.
    /// @dev The function will revert if the payload length is invalid.
    ///     Update limit payload is 9 bytes.
    ///     byte 0       : chain ID
    ///     bytes 1-8    : new limit
    /// @param _payload The payload to be decoded.
    /// @return senderChainID the sending chain ID to update the limit of.
    /// @return newNonce the new nonce of the sending chain ID.
    function decodeUpdateMinLimitNoncePayload(bytes memory _payload)
        internal
        pure
        returns (uint8 senderChainID, uint64 newNonce)
    {
        require(_payload.length == 9, "BridgeUtils: Invalid payload length");
        senderChainID = uint8(_payload[0]);

        // Extracts the uint64 value by loading 32 bytes starting just after the first byte.
        // Position uint64 to the least significant bits by shifting it 192 bits to the right.
        assembly {
            newNonce := shr(192, mload(add(add(_payload, 0x20), 1)))
        }
    }
}
