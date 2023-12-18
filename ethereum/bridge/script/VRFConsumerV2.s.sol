// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.7;

import "forge-std/Script.sol";
import "../src/VRFConsumerV2.sol";
import "./HelperConfig.sol";
import "../src/test/mocks/LinkToken.sol";
import "../src/test/mocks/MockVRFCoordinatorV2.sol";

contract DeployVRFConsumerV2 is Script, HelperConfig {
    function run() external {
        HelperConfig helperConfig = new HelperConfig();

        (
            ,
            ,
            ,
            address link,
            ,
            ,
            uint64 subscriptionId,
            address vrfCoordinator,
            bytes32 keyHash
        ) = helperConfig.activeNetworkConfig();

        if (link == address(0)) {
            link = address(new LinkToken());
        }

        if (vrfCoordinator == address(0)) {
            vrfCoordinator = address(new MockVRFCoordinatorV2());
        }

        vm.startBroadcast();

        new VRFConsumerV2(
            subscriptionId,
            vrfCoordinator,
            link,
            keyHash
        );
        
        vm.stopBroadcast();
    }
}
