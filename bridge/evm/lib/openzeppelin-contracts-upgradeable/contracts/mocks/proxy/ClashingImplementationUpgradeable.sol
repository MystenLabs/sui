// SPDX-License-Identifier: MIT

pragma solidity ^0.8.20;
import {Initializable} from "../../proxy/utils/Initializable.sol";

/**
 * @dev Implementation contract with a payable changeAdmin(address) function made to clash with
 * TransparentUpgradeableProxy's to test correct functioning of the Transparent Proxy feature.
 */
contract ClashingImplementationUpgradeable is Initializable {
    event ClashingImplementationCall();

    function __ClashingImplementation_init() internal onlyInitializing {
    }

    function __ClashingImplementation_init_unchained() internal onlyInitializing {
    }
    function upgradeToAndCall(address, bytes calldata) external payable {
        emit ClashingImplementationCall();
    }

    function delegatedFunction() external pure returns (bool) {
        return true;
    }
}
