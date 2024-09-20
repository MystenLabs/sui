// SPDX-License-Identifier: MIT

pragma solidity ^0.8.20;
import {Initializable} from "../../proxy/utils/Initializable.sol";

contract ERC165MaliciousDataUpgradeable is Initializable {
    function __ERC165MaliciousData_init() internal onlyInitializing {
    }

    function __ERC165MaliciousData_init_unchained() internal onlyInitializing {
    }
    function supportsInterface(bytes4) public pure returns (bool) {
        assembly {
            mstore(0, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff)
            return(0, 32)
        }
    }
}
