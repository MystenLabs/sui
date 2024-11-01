// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title ISuiBridge
/// @dev Interface for the Sui Bridge contract.
interface ISuiBridge {
    /// @notice Emitted when tokens are deposited to be bridged.
    /// @param sourceChainID The ID of the source chain (this chain).
    /// @param nonce The nonce of the transaction on source chain.
    /// @param destinationChainID The ID of the destination chain.
    /// @param tokenID The code of the token.
    /// @param suiAdjustedAmount The amount of tokens to transfer, adjusted for Sui decimals.
    /// @param senderAddress The address of the sender.
    /// @param recipientAddress The address of the sender.
    event TokensDeposited(
        uint8 indexed sourceChainID,
        uint64 indexed nonce,
        uint8 indexed destinationChainID,
        uint8 tokenID,
        uint64 suiAdjustedAmount,
        address senderAddress,
        bytes recipientAddress
    );

    /// @notice Emitted when bridged tokens are transferred to the recipient address.
    /// @param sourceChainID The ID of the source chain.
    /// @param nonce The nonce of the transaction on source chain.
    /// @param destinationChainID The ID of the destination chain (this chain).
    /// @param tokenID The code of the token.
    /// @param erc20AdjustedAmount The amount of tokens claimed, adjusted for ERC20 decimals.
    /// @param senderAddress The address of the sender.
    /// @param recipientAddress The address of the sender.
    // event BridgedTokensTransferred(
    event TokensClaimed(
        uint8 indexed sourceChainID,
        uint64 indexed nonce,
        uint8 indexed destinationChainID,
        uint8 tokenID,
        uint256 erc20AdjustedAmount,
        bytes senderAddress,
        address recipientAddress
    );

    /// @notice Emitted when the bridge is paused or unpaused.
    /// @param nonce The governance action nonce.
    /// @param paused A boolean indicating whether the bridge is paused or not.
    event EmergencyOperation(uint64 nonce, bool paused);
}
