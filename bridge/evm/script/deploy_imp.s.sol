// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "forge-std/Script.sol";
import "openzeppelin-foundry-upgrades/Upgrades.sol";
import {Upgrades} from "openzeppelin-foundry-upgrades/Upgrades.sol";
import "openzeppelin-foundry-upgrades/Options.sol";
import "@openzeppelin/contracts/utils/Strings.sol";
import "../contracts/BridgeCommittee.sol";
import "../contracts/BridgeVault.sol";
import "../contracts/BridgeConfig.sol";
import "../contracts/BridgeLimiter.sol";
import "../contracts/SuiBridge.sol";
import "../test/mocks/MockTokens.sol";

contract DeployImplimentation is Script {
    function run() external {
        uint256 deployerPrivateKey = vm.envUint("PRIVATE_KEY");
        vm.startBroadcast(deployerPrivateKey);

        string memory referenceContractName = vm.envString("UPGRADE_REFERENCE_CONTRACT_NAME");
        string memory newContractName = vm.envString("UPGRADE_NEW_CONTRACT_NAME");

        // check if the reference contract is valid
        Options memory opts;
        opts.referenceContract = referenceContractName;
        Upgrades.validateUpgrade(newContractName, opts);

        // reset reference contract
        opts.referenceContract = "";
        address implimentation = Upgrades.deployImplementation(newContractName, opts);
        console.log("New implimentation contract deployed at: %s", implimentation);

        vm.stopBroadcast();
    }

    // used to ignore for forge coverage
    function testSkip() public {}
}
