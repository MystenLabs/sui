// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ERC20Upgradeable} from "../../token/ERC20/ERC20Upgradeable.sol";
import {Initializable} from "../../proxy/utils/Initializable.sol";

abstract contract ERC20ApprovalMockUpgradeable is Initializable, ERC20Upgradeable {
    function __ERC20ApprovalMock_init() internal onlyInitializing {
    }

    function __ERC20ApprovalMock_init_unchained() internal onlyInitializing {
    }
    function _approve(address owner, address spender, uint256 amount, bool) internal virtual override {
        super._approve(owner, spender, amount, true);
    }
}
