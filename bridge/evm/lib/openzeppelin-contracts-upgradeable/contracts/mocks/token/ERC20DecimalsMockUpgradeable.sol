// SPDX-License-Identifier: MIT

pragma solidity ^0.8.20;

import {ERC20Upgradeable} from "../../token/ERC20/ERC20Upgradeable.sol";
import {Initializable} from "../../proxy/utils/Initializable.sol";

abstract contract ERC20DecimalsMockUpgradeable is Initializable, ERC20Upgradeable {
    uint8 private _decimals;

    function __ERC20DecimalsMock_init(uint8 decimals_) internal onlyInitializing {
        __ERC20DecimalsMock_init_unchained(decimals_);
    }

    function __ERC20DecimalsMock_init_unchained(uint8 decimals_) internal onlyInitializing {
        _decimals = decimals_;
    }

    function decimals() public view override returns (uint8) {
        return _decimals;
    }
}
