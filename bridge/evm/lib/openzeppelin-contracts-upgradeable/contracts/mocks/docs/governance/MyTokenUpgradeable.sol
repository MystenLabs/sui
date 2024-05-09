// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ERC20Upgradeable} from "../../../token/ERC20/ERC20Upgradeable.sol";
import {ERC20PermitUpgradeable} from "../../../token/ERC20/extensions/ERC20PermitUpgradeable.sol";
import {ERC20VotesUpgradeable} from "../../../token/ERC20/extensions/ERC20VotesUpgradeable.sol";
import {NoncesUpgradeable} from "../../../utils/NoncesUpgradeable.sol";
import {Initializable} from "../../../proxy/utils/Initializable.sol";

contract MyTokenUpgradeable is Initializable, ERC20Upgradeable, ERC20PermitUpgradeable, ERC20VotesUpgradeable {
    function __MyToken_init() internal onlyInitializing {
        __ERC20_init_unchained("MyToken", "MTK");
        __EIP712_init_unchained("MyToken", "1");
        __ERC20Permit_init_unchained("MyToken");
    }

    function __MyToken_init_unchained() internal onlyInitializing {}

    // The functions below are overrides required by Solidity.

    function _update(address from, address to, uint256 amount) internal override(ERC20Upgradeable, ERC20VotesUpgradeable) {
        super._update(from, to, amount);
    }

    function nonces(address owner) public view virtual override(ERC20PermitUpgradeable, NoncesUpgradeable) returns (uint256) {
        return super.nonces(owner);
    }
}
