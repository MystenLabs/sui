// SPDX-License-Identifier: MIT

pragma solidity ^0.8.20;

import {IERC165} from "@openzeppelin/contracts/utils/introspection/IERC165.sol";
import {Initializable} from "../../proxy/utils/Initializable.sol";

contract ERC165ReturnBombMockUpgradeable is Initializable, IERC165 {
    function __ERC165ReturnBombMock_init() internal onlyInitializing {
    }

    function __ERC165ReturnBombMock_init_unchained() internal onlyInitializing {
    }
    function supportsInterface(bytes4 interfaceId) public pure override returns (bool) {
        if (interfaceId == type(IERC165).interfaceId) {
            assembly {
                mstore(0, 1)
            }
        }
        assembly {
            return(0, 101500)
        }
    }
}
