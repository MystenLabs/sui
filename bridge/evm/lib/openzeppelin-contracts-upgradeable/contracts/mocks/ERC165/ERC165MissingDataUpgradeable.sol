// SPDX-License-Identifier: MIT

pragma solidity ^0.8.20;
import {Initializable} from "../../proxy/utils/Initializable.sol";

contract ERC165MissingDataUpgradeable is Initializable {
    function __ERC165MissingData_init() internal onlyInitializing {
    }

    function __ERC165MissingData_init_unchained() internal onlyInitializing {
    }
    function supportsInterface(bytes4 interfaceId) public view {} // missing return
}
