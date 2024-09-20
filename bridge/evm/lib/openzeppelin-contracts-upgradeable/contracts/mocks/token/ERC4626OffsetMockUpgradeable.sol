// SPDX-License-Identifier: MIT

pragma solidity ^0.8.20;

import {ERC4626Upgradeable} from "../../token/ERC20/extensions/ERC4626Upgradeable.sol";
import {Initializable} from "../../proxy/utils/Initializable.sol";

abstract contract ERC4626OffsetMockUpgradeable is Initializable, ERC4626Upgradeable {
    uint8 private _offset;

    function __ERC4626OffsetMock_init(uint8 offset_) internal onlyInitializing {
        __ERC4626OffsetMock_init_unchained(offset_);
    }

    function __ERC4626OffsetMock_init_unchained(uint8 offset_) internal onlyInitializing {
        _offset = offset_;
    }

    function _decimalsOffset() internal view virtual override returns (uint8) {
        return _offset;
    }
}
