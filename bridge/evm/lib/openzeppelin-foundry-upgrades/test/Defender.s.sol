// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Script} from "forge-std/Script.sol";
import {console} from "forge-std/console.sol";

import {Defender} from "openzeppelin-foundry-upgrades/Defender.sol";

/**
 * @dev Sample script to deploy a contract using Defender.
 */
contract DefenderScript is Script {
    function setUp() public {}

    function run() public {
        address deployed = Defender.deployContract("WithConstructor.sol", abi.encode(123));
        console.log("Deployed contract to address", deployed);
    }
}
