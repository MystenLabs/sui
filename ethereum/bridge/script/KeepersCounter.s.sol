// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.7;

import "forge-std/Script.sol";
import "../src/KeepersCounter.sol";
import "./HelperConfig.sol";

contract DeployKeepersCounter is Script, HelperConfig {
    function run() external {
        HelperConfig helperConfig = new HelperConfig();

        (, , , , uint256 updateInterval, , , , ) = helperConfig
            .activeNetworkConfig();

        vm.startBroadcast();

        new KeepersCounter(updateInterval);

        vm.stopBroadcast();
    }
}
