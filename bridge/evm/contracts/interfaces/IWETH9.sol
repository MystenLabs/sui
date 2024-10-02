// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "@openzeppelin/contracts/token/ERC20/IERC20.sol";

/// @title IWETH9
/// @notice Interface for the WETH9 contract.
interface IWETH9 is IERC20 {
    /// @notice Deposit ETH to get wrapped ETH
    /// @dev This function enables users to deposit ETH and receive wrapped ETH tokens in return.
    /// @dev The amount of ETH to be deposited should be sent along with the function call.
    function deposit() external payable;

    /// @notice Withdraw wrapped ETH to get ETH
    /// @dev This function allows users to withdraw a specified amount of wrapped ETH and receive ETH in return.
    /// @param wad The amount of wrapped ETH to be withdrawn.
    function withdraw(uint256 wad) external;
}
