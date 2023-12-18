// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.7;

import "forge-std/Script.sol";
import "../src/APIConsumer.sol";
import "./HelperConfig.sol";
import "../src/test/mocks/MockOracle.sol";
import "../src/test/mocks/LinkToken.sol";

contract DeployAPIConsumer is Script, HelperConfig {
    function run() external {
        HelperConfig helperConfig = new HelperConfig();

        (
            address oracle,
            bytes32 jobId,
            uint256 chainlinkFee,
            address link,
            ,
            ,
            ,
            ,

        ) = helperConfig.activeNetworkConfig();

        if (link == address(0)) {
            link = address(new LinkToken());
        }

        if (oracle == address(0)) {
            oracle = address(new MockOracle(link));
        }

        vm.startBroadcast();

        new APIConsumer(oracle, jobId, chainlinkFee, link);

        vm.stopBroadcast();
    }
}
