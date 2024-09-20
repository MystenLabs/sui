// SPDX-License-Identifier: MIT

pragma solidity ^0.8.20;

import {ERC20Upgradeable} from "../../token/ERC20/ERC20Upgradeable.sol";
import {Initializable} from "../../proxy/utils/Initializable.sol";

abstract contract ERC20ReturnFalseMockUpgradeable is Initializable, ERC20Upgradeable {
    function __ERC20ReturnFalseMock_init() internal onlyInitializing {
    }

    function __ERC20ReturnFalseMock_init_unchained() internal onlyInitializing {
    }
    function transfer(address, uint256) public pure override returns (bool) {
        return false;
    }

    function transferFrom(address, address, uint256) public pure override returns (bool) {
        return false;
    }

    function approve(address, uint256) public pure override returns (bool) {
        return false;
    }
}
