// SPDX-License-Identifier: MIT

pragma solidity ^0.8.20;

import {OwnableUpgradeable} from "../access/OwnableUpgradeable.sol";
import {IERC1271} from "@openzeppelin/contracts/interfaces/IERC1271.sol";
import {ECDSA} from "@openzeppelin/contracts/utils/cryptography/ECDSA.sol";
import {Initializable} from "../proxy/utils/Initializable.sol";

contract ERC1271WalletMockUpgradeable is Initializable, OwnableUpgradeable, IERC1271 {
    function __ERC1271WalletMock_init(address originalOwner) internal onlyInitializing {
        __Ownable_init_unchained(originalOwner);
    }

    function __ERC1271WalletMock_init_unchained(address) internal onlyInitializing {}

    function isValidSignature(bytes32 hash, bytes memory signature) public view returns (bytes4 magicValue) {
        return ECDSA.recover(hash, signature) == owner() ? this.isValidSignature.selector : bytes4(0);
    }
}

contract ERC1271MaliciousMockUpgradeable is Initializable, IERC1271 {
    function __ERC1271MaliciousMock_init() internal onlyInitializing {
    }

    function __ERC1271MaliciousMock_init_unchained() internal onlyInitializing {
    }
    function isValidSignature(bytes32, bytes memory) public pure returns (bytes4) {
        assembly {
            mstore(0, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff)
            return(0, 32)
        }
    }
}
