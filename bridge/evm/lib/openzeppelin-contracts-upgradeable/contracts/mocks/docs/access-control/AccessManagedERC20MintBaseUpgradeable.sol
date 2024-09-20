// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {AccessManagedUpgradeable} from "../../../access/manager/AccessManagedUpgradeable.sol";
import {ERC20Upgradeable} from "../../../token/ERC20/ERC20Upgradeable.sol";
import {Initializable} from "../../../proxy/utils/Initializable.sol";

contract AccessManagedERC20MintUpgradeable is Initializable, ERC20Upgradeable, AccessManagedUpgradeable {
    function __AccessManagedERC20Mint_init(address manager) internal onlyInitializing {
        __ERC20_init_unchained("MyToken", "TKN");
        __AccessManaged_init_unchained(manager);
    }

    function __AccessManagedERC20Mint_init_unchained(address) internal onlyInitializing {}

    // Minting is restricted according to the manager rules for this function.
    // The function is identified by its selector: 0x40c10f19.
    // Calculated with bytes4(keccak256('mint(address,uint256)'))
    function mint(address to, uint256 amount) public restricted {
        _mint(to, amount);
    }
}
