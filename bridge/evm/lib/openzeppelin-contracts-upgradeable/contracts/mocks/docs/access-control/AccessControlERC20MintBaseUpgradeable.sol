// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {AccessControlUpgradeable} from "../../../access/AccessControlUpgradeable.sol";
import {ERC20Upgradeable} from "../../../token/ERC20/ERC20Upgradeable.sol";
import {Initializable} from "../../../proxy/utils/Initializable.sol";

contract AccessControlERC20MintBaseUpgradeable is Initializable, ERC20Upgradeable, AccessControlUpgradeable {
    // Create a new role identifier for the minter role
    bytes32 public constant MINTER_ROLE = keccak256("MINTER_ROLE");

    error CallerNotMinter(address caller);

    function __AccessControlERC20MintBase_init(address minter) internal onlyInitializing {
        __ERC20_init_unchained("MyToken", "TKN");
        __AccessControlERC20MintBase_init_unchained(minter);
    }

    function __AccessControlERC20MintBase_init_unchained(address minter) internal onlyInitializing {
        // Grant the minter role to a specified account
        _grantRole(MINTER_ROLE, minter);
    }

    function mint(address to, uint256 amount) public {
        // Check that the calling account has the minter role
        if (!hasRole(MINTER_ROLE, msg.sender)) {
            revert CallerNotMinter(msg.sender);
        }
        _mint(to, amount);
    }
}
