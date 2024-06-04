// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "forge-std/Script.sol";
// import "openzeppelin-foundry-upgrades/Upgrades.sol";
import {Upgrades} from "openzeppelin-foundry-upgrades/Upgrades.sol";
import "openzeppelin-foundry-upgrades/Options.sol";
import "@openzeppelin/contracts/utils/Strings.sol";
import "../contracts/BridgeCommittee.sol";
import "../contracts/BridgeVault.sol";
import "../contracts/BridgeConfig.sol";
import "../contracts/BridgeLimiter.sol";
import "../contracts/SuiBridge.sol";
import "../test/mocks/MockTokens.sol";

contract DeployBridge is Script {
    function run() external {
        uint256 deployerPrivateKey = vm.envUint("PRIVATE_KEY");
        vm.startBroadcast(deployerPrivateKey);

        Options memory opts;
        opts.referenceContract = "SuiBridgeV1.sol";
        Upgrades.validateUpgrade("SuiBridge.sol", opts);

        opts.referenceContract = "";
        address sui_bridge = Upgrades.deployImplementation("SuiBridge.sol", opts);
        console.log("SuiBridge deployed at: %s", sui_bridge);

        vm.stopBroadcast();
    }

    // used to ignore for forge coverage
    function testSkip() public {}
}
