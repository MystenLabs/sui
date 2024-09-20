// SPDX-License-Identifier: MIT

pragma solidity ^0.8.20;

import {Arrays} from "@openzeppelin/contracts/utils/Arrays.sol";
import {Initializable} from "../proxy/utils/Initializable.sol";

contract Uint256ArraysMockUpgradeable is Initializable {
    using Arrays for uint256[];

    uint256[] private _array;

    function __Uint256ArraysMock_init(uint256[] memory array) internal onlyInitializing {
        __Uint256ArraysMock_init_unchained(array);
    }

    function __Uint256ArraysMock_init_unchained(uint256[] memory array) internal onlyInitializing {
        _array = array;
    }

    function findUpperBound(uint256 element) external view returns (uint256) {
        return _array.findUpperBound(element);
    }

    function unsafeAccess(uint256 pos) external view returns (uint256) {
        return _array.unsafeAccess(pos).value;
    }
}

contract AddressArraysMockUpgradeable is Initializable {
    using Arrays for address[];

    address[] private _array;

    function __AddressArraysMock_init(address[] memory array) internal onlyInitializing {
        __AddressArraysMock_init_unchained(array);
    }

    function __AddressArraysMock_init_unchained(address[] memory array) internal onlyInitializing {
        _array = array;
    }

    function unsafeAccess(uint256 pos) external view returns (address) {
        return _array.unsafeAccess(pos).value;
    }
}

contract Bytes32ArraysMockUpgradeable is Initializable {
    using Arrays for bytes32[];

    bytes32[] private _array;

    function __Bytes32ArraysMock_init(bytes32[] memory array) internal onlyInitializing {
        __Bytes32ArraysMock_init_unchained(array);
    }

    function __Bytes32ArraysMock_init_unchained(bytes32[] memory array) internal onlyInitializing {
        _array = array;
    }

    function unsafeAccess(uint256 pos) external view returns (bytes32) {
        return _array.unsafeAccess(pos).value;
    }
}
