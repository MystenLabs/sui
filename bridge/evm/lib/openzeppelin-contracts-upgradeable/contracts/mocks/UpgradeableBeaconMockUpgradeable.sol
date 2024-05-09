// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {IBeacon} from "@openzeppelin/contracts/proxy/beacon/IBeacon.sol";
import {Initializable} from "../proxy/utils/Initializable.sol";

contract UpgradeableBeaconMockUpgradeable is Initializable, IBeacon {
    address public implementation;

    function __UpgradeableBeaconMock_init(address impl) internal onlyInitializing {
        __UpgradeableBeaconMock_init_unchained(impl);
    }

    function __UpgradeableBeaconMock_init_unchained(address impl) internal onlyInitializing {
        implementation = impl;
    }
}

import { IProxyExposed } from "@openzeppelin/contracts/mocks/UpgradeableBeaconMock.sol";

contract UpgradeableBeaconReentrantMockUpgradeable is Initializable, IBeacon {
    error BeaconProxyBeaconSlotAddress(address beacon);

    function __UpgradeableBeaconReentrantMock_init() internal onlyInitializing {
    }

    function __UpgradeableBeaconReentrantMock_init_unchained() internal onlyInitializing {
    }
    function implementation() external view override returns (address) {
        // Revert with the beacon seen in the proxy at the moment of calling to check if it's
        // set before the call.
        revert BeaconProxyBeaconSlotAddress(IProxyExposed(msg.sender).$getBeacon());
    }
}
