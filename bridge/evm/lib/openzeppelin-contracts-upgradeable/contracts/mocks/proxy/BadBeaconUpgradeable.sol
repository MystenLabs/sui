// SPDX-License-Identifier: MIT

pragma solidity ^0.8.20;
import {Initializable} from "../../proxy/utils/Initializable.sol";

contract BadBeaconNoImplUpgradeable is Initializable {    function __BadBeaconNoImpl_init() internal onlyInitializing {
    }

    function __BadBeaconNoImpl_init_unchained() internal onlyInitializing {
    }
}

contract BadBeaconNotContractUpgradeable is Initializable {
    function __BadBeaconNotContract_init() internal onlyInitializing {
    }

    function __BadBeaconNotContract_init_unchained() internal onlyInitializing {
    }
    function implementation() external pure returns (address) {
        return address(0x1);
    }
}
