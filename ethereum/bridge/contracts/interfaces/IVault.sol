// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.20;

import "./ICommon.sol";

interface IVault is ICommon {
    function batchTransferToErc20(Erc20Transfer[] calldata _transfers) external;
}
