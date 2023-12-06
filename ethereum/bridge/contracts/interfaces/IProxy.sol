// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.20;

interface IProxy {
    function completeContractInit() external;

    function upgradeContract(string memory name, address addr) external;

    function addContract(string memory name, address addr) external;

    function getContract(string memory name) external view returns (address);
}
