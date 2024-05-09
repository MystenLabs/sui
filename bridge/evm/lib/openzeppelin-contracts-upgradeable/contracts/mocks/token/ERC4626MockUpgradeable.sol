// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {ERC20Upgradeable} from "../../token/ERC20/ERC20Upgradeable.sol";
import {ERC4626Upgradeable} from "../../token/ERC20/extensions/ERC4626Upgradeable.sol";
import {Initializable} from "../../proxy/utils/Initializable.sol";

contract ERC4626MockUpgradeable is Initializable, ERC4626Upgradeable {
    function __ERC4626Mock_init(address underlying) internal onlyInitializing {
        __ERC20_init_unchained("ERC4626Mock", "E4626M");
        __ERC4626_init_unchained(IERC20(underlying));
    }

    function __ERC4626Mock_init_unchained(address) internal onlyInitializing {}

    function mint(address account, uint256 amount) external {
        _mint(account, amount);
    }

    function burn(address account, uint256 amount) external {
        _burn(account, amount);
    }
}
