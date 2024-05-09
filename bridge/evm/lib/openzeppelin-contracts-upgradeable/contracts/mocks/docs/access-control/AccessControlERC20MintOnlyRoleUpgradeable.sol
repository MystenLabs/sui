// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {AccessControlUpgradeable} from "../../../access/AccessControlUpgradeable.sol";
import {ERC20Upgradeable} from "../../../token/ERC20/ERC20Upgradeable.sol";
import {Initializable} from "../../../proxy/utils/Initializable.sol";

contract AccessControlERC20MintUpgradeable is Initializable, ERC20Upgradeable, AccessControlUpgradeable {
    bytes32 public constant MINTER_ROLE = keccak256("MINTER_ROLE");
    bytes32 public constant BURNER_ROLE = keccak256("BURNER_ROLE");

    function __AccessControlERC20Mint_init(address minter, address burner) internal onlyInitializing {
        __ERC20_init_unchained("MyToken", "TKN");
        __AccessControlERC20Mint_init_unchained(minter, burner);
    }

    function __AccessControlERC20Mint_init_unchained(address minter, address burner) internal onlyInitializing {
        _grantRole(MINTER_ROLE, minter);
        _grantRole(BURNER_ROLE, burner);
    }

    function mint(address to, uint256 amount) public onlyRole(MINTER_ROLE) {
        _mint(to, amount);
    }

    function burn(address from, uint256 amount) public onlyRole(BURNER_ROLE) {
        _burn(from, amount);
    }
}
