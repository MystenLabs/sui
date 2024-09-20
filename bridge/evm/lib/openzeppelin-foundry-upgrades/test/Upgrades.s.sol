// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Script} from "forge-std/Script.sol";

import {Greeter} from "./contracts/Greeter.sol";
import {GreeterProxiable} from "./contracts/GreeterProxiable.sol";
import {GreeterV2} from "./contracts/GreeterV2.sol";
import {GreeterV2Proxiable} from "./contracts/GreeterV2Proxiable.sol";

import {Upgrades} from "openzeppelin-foundry-upgrades/Upgrades.sol";

/**
 * @dev Sample script to deploy and upgrade contracts using transparent, UUPS, and beacon proxies.
 */
contract UpgradesScript is Script {
    function setUp() public {}

    function run() public {
        vm.startBroadcast();

        // example deployment and upgrade of a transparent proxy
        address transparentProxy = Upgrades.deployTransparentProxy(
            "Greeter.sol",
            msg.sender,
            abi.encodeCall(Greeter.initialize, ("hello"))
        );
        Upgrades.upgradeProxy(transparentProxy, "GreeterV2.sol", abi.encodeCall(GreeterV2.resetGreeting, ()));

        // example deployment and upgrade of a UUPS proxy
        address uupsProxy = Upgrades.deployUUPSProxy(
            "GreeterProxiable.sol",
            abi.encodeCall(GreeterProxiable.initialize, ("hello"))
        );
        Upgrades.upgradeProxy(
            uupsProxy,
            "GreeterV2Proxiable.sol",
            abi.encodeCall(GreeterV2Proxiable.resetGreeting, ())
        );

        // example deployment of a beacon proxy and upgrade of the beacon
        address beacon = Upgrades.deployBeacon("Greeter.sol", msg.sender);
        Upgrades.deployBeaconProxy(beacon, abi.encodeCall(Greeter.initialize, ("hello")));
        Upgrades.upgradeBeacon(beacon, "GreeterV2.sol");

        vm.stopBroadcast();
    }
}
