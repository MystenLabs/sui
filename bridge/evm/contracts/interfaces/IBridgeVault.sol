// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title IBridgeVault
/// @dev Interface for the BridgeVault contract.
interface IBridgeVault {
    /// @notice Transfers ERC20 tokens from the BridgeVault contract to a target address.
    /// @param tokenAddress The address of the ERC20 token.
    /// @param targetAddress The address to transfer the tokens to.
    /// @param amount The amount of tokens to transfer.
    function transferERC20(address tokenAddress, address targetAddress, uint256 amount) external;

    /// @notice Transfers ETH from the BridgeVault contract to a target address.
    /// @param targetAddress The address to transfer the ETH to.
    /// @param amount The amount of ETH to transfer.
    function transferETH(address payable targetAddress, uint256 amount) external;
}
