// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "@openzeppelin/contracts/access/Ownable.sol";
import "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";
import "@openzeppelin/contracts/utils/ReentrancyGuard.sol";
import "./interfaces/IBridgeVault.sol";
import "./interfaces/IWETH9.sol";

/// @title BridgeVault
/// @notice A contract that acts as a vault for transferring ERC20 tokens and ETH. It enables the owner
/// (intended to be the SuiBridge contract) to transfer tokens to a target address. It also supports
/// unwrapping WETH (Wrapped Ether) and transferring the unwrapped ETH.
/// @dev The contract is initialized with the deployer as the owner. The ownership is intended to be
/// transferred to the SuiBridge contract after the bridge contract is deployed.
contract BridgeVault is Ownable, IBridgeVault, ReentrancyGuard {
    /* ========== STATE VARIABLES ========== */

    IWETH9 public immutable wETH;

    /* ========== CONSTRUCTOR ========== */

    /// @notice Constructor function for the BridgeVault contract.
    /// @param _wETH The address of the Wrapped Ether (WETH) contract.
    constructor(address _wETH) Ownable(msg.sender) ReentrancyGuard() {
        // Set the WETH address
        wETH = IWETH9(_wETH);
    }

    /// @notice Transfers ERC20 tokens from the contract to a target address. Only the owner of
    /// the contract can call this function.
    /// @dev This function is intended to only be called by the SuiBridge contract.
    /// @param tokenAddress The address of the ERC20 token.
    /// @param recipientAddress The address to transfer the tokens to.
    /// @param amount The amount of tokens to transfer.
    function transferERC20(address tokenAddress, address recipientAddress, uint256 amount)
        external
        override
        onlyOwner
        nonReentrant
    {
        // Transfer the tokens from the contract to the target address
        SafeERC20.safeTransfer(IERC20(tokenAddress), recipientAddress, amount);
    }

    /// @notice Unwraps stored wrapped ETH and transfers the newly withdrawn ETH to the provided target
    /// address. Only the owner of the contract can call this function.
    /// @dev This function is intended to only be called by the SuiBridge contract.
    /// @param recipientAddress The address to transfer the ETH to.
    /// @param amount The amount of ETH to transfer.
    function transferETH(address payable recipientAddress, uint256 amount)
        external
        override
        onlyOwner
        nonReentrant
    {
        // Unwrap the WETH
        wETH.withdraw(amount);

        // Transfer the unwrapped ETH to the target address
        (bool success,) = recipientAddress.call{value: amount}("");
        require(success, "ETH transfer failed");
    }

    /// @notice Wraps as eth sent to this contract.
    /// @dev skip if sender is wETH contract to avoid infinite loop.
    receive() external payable {
        if (msg.sender != address(wETH)) {
            wETH.deposit{value: msg.value}();
        }
    }
}
