// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "../../utils/MessageVerifier.sol";
import "../interfaces/IBridgeCommitteeV2.sol";

/// @title MessageVerifier
/// @notice This contract provides an interface to verify messages and their signatures
/// using a BridgeCommittee contract. This contract is also responsible for maintaining
/// nonces for each message type to prevent replay attacks.
/// @dev The contract is intended to be inherited by contracts that require message and signature
/// verification.
abstract contract MessageVerifierV2 is MessageVerifier {
    IBridgeCommitteeV2 public committeeV2;

    /* ========== INITIALIZER ========== */

    function __MessageVerifierV2_init() internal onlyInitializing {
        committeeV2 = IBridgeCommitteeV2(address(committee));
    }

    /* ========== MODIFIERS ========== */

    /// @notice Verifies the provided message and signatures using the BridgeCommittee contract.
    /// @dev The function will revert if the message type does not match the expected type,
    /// if the signatures are invalid, or if the message nonce is invalid.
    /// @param message The BridgeUtils to be verified.
    /// @param signatures The array of signatures to be verified.
    /// @param messageType The expected message type of the provided message.
    modifier verifyMessageAndSignaturesV2(
        BridgeUtils.Message memory message,
        bytes[] memory signatures,
        uint8 epoch,
        uint8 messageType
    ) {
        // verify message type
        require(message.messageType == messageType, "MessageVerifier: message does not match type");
        // verify signatures
        committeeV2.verifySignaturesV2(epoch, signatures, message);
        // increment message type nonce
        if (messageType != BridgeUtils.TOKEN_TRANSFER) {
            // verify chain ID
            require(
                message.chainID == committee.config().chainID(), "MessageVerifier: Invalid chain ID"
            );
            require(message.nonce == nonces[message.messageType], "MessageVerifier: Invalid nonce");
            nonces[message.messageType]++;
        }
        _;
    }
}
