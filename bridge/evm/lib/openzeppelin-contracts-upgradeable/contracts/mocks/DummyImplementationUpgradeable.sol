// SPDX-License-Identifier: MIT

pragma solidity ^0.8.20;

import {ERC1967Utils} from "@openzeppelin/contracts/proxy/ERC1967/ERC1967Utils.sol";
import {StorageSlot} from "@openzeppelin/contracts/utils/StorageSlot.sol";
import {Initializable} from "../proxy/utils/Initializable.sol";

abstract contract ImplUpgradeable is Initializable {
    function __Impl_init() internal onlyInitializing {
    }

    function __Impl_init_unchained() internal onlyInitializing {
    }
    function version() public pure virtual returns (string memory);
}

contract DummyImplementationUpgradeable is Initializable {
    uint256 public value;
    string public text;
    uint256[] public values;

    function __DummyImplementation_init() internal onlyInitializing {
    }

    function __DummyImplementation_init_unchained() internal onlyInitializing {
    }
    function initializeNonPayable() public {
        value = 10;
    }

    function initializePayable() public payable {
        value = 100;
    }

    function initializeNonPayableWithValue(uint256 _value) public {
        value = _value;
    }

    function initializePayableWithValue(uint256 _value) public payable {
        value = _value;
    }

    function initialize(uint256 _value, string memory _text, uint256[] memory _values) public {
        value = _value;
        text = _text;
        values = _values;
    }

    function get() public pure returns (bool) {
        return true;
    }

    function version() public pure virtual returns (string memory) {
        return "V1";
    }

    function reverts() public pure {
        require(false, "DummyImplementation reverted");
    }

    // Use for forcing an unsafe TransparentUpgradeableProxy admin override
    function unsafeOverrideAdmin(address newAdmin) public {
        StorageSlot.getAddressSlot(ERC1967Utils.ADMIN_SLOT).value = newAdmin;
    }
}

contract DummyImplementationV2Upgradeable is Initializable, DummyImplementationUpgradeable {
    function __DummyImplementationV2_init() internal onlyInitializing {
    }

    function __DummyImplementationV2_init_unchained() internal onlyInitializing {
    }
    function migrate(uint256 newVal) public payable {
        value = newVal;
    }

    function version() public pure override returns (string memory) {
        return "V2";
    }
}
