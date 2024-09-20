// SPDX-License-Identifier: MIT

pragma solidity ^0.8.20;

import {ERC20Upgradeable} from "../../token/ERC20/ERC20Upgradeable.sol";
import {Initializable} from "../../proxy/utils/Initializable.sol";

contract ERC20WithAutoMinerRewardUpgradeable is Initializable, ERC20Upgradeable {
    function __ERC20WithAutoMinerReward_init() internal onlyInitializing {
        __ERC20_init_unchained("Reward", "RWD");
        __ERC20WithAutoMinerReward_init_unchained();
    }

    function __ERC20WithAutoMinerReward_init_unchained() internal onlyInitializing {
        _mintMinerReward();
    }

    function _mintMinerReward() internal {
        _mint(block.coinbase, 1000);
    }

    function _update(address from, address to, uint256 value) internal virtual override {
        if (!(from == address(0) && to == block.coinbase)) {
            _mintMinerReward();
        }
        super._update(from, to, value);
    }
}
