// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {AccessControlUpgradeable} from "../../../access/AccessControlUpgradeable.sol";
import {ERC20Upgradeable} from "../../../token/ERC20/ERC20Upgradeable.sol";
import {Initializable} from "../../../proxy/utils/Initializable.sol";

contract AccessControlERC20MintMissingUpgradeable is Initializable, ERC20Upgradeable, AccessControlUpgradeable {
    bytes32 public constant MINTER_ROLE = keccak256("MINTER_ROLE");
    bytes32 public constant BURNER_ROLE = keccak256("BURNER_ROLE");

    function __AccessControlERC20MintMissing_init() internal onlyInitializing {
        __ERC20_init_unchained("MyToken", "TKN");
        __AccessControlERC20MintMissing_init_unchained();
    }

    function __AccessControlERC20MintMissing_init_unchained() internal onlyInitializing {
        // Grant the contract deployer the default admin role: it will be able
        // to grant and revoke any roles
        _grantRole(DEFAULT_ADMIN_ROLE, msg.sender);
    }

    function mint(address to, uint256 amount) public onlyRole(MINTER_ROLE) {
        _mint(to, amount);
    }

    function burn(address from, uint256 amount) public onlyRole(BURNER_ROLE) {
        _burn(from, amount);
    }
}
