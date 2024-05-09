// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Script} from "forge-std/Script.sol";
import {console} from "forge-std/console.sol";

import {WithConstructor} from "./contracts/WithConstructor.sol";

import {Defender, DefenderOptions} from "openzeppelin-foundry-upgrades/Defender.sol";

/**
 * @dev Sample script to deploy a contract using Defender.
 */
contract DefenderScript is Script {
    function setUp() public {}

    function run() public {
        DefenderOptions memory opts;
        // Add options here
        address deployed = Defender.deployContract("WithConstructor.sol", abi.encode(123), opts);
        console.log("Deployed contract to address", deployed);
    }
}
