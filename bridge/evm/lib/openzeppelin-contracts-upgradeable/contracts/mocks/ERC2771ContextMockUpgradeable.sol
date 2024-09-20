// SPDX-License-Identifier: MIT

pragma solidity ^0.8.20;

import {ContextMockUpgradeable} from "./ContextMockUpgradeable.sol";
import {ContextUpgradeable} from "../utils/ContextUpgradeable.sol";
import {MulticallUpgradeable} from "../utils/MulticallUpgradeable.sol";
import {ERC2771ContextUpgradeable} from "../metatx/ERC2771ContextUpgradeable.sol";
import {Initializable} from "../proxy/utils/Initializable.sol";

// By inheriting from ERC2771Context, Context's internal functions are overridden automatically
contract ERC2771ContextMockUpgradeable is Initializable, ContextMockUpgradeable, ERC2771ContextUpgradeable, MulticallUpgradeable {
    /// @custom:oz-upgrades-unsafe-allow constructor
    constructor(address trustedForwarder) ERC2771ContextUpgradeable(trustedForwarder) {
        emit Sender(_msgSender()); // _msgSender() should be accessible during construction
    }

    function _msgSender() internal view override(ContextUpgradeable, ERC2771ContextUpgradeable) returns (address) {
        return ERC2771ContextUpgradeable._msgSender();
    }

    function _msgData() internal view override(ContextUpgradeable, ERC2771ContextUpgradeable) returns (bytes calldata) {
        return ERC2771ContextUpgradeable._msgData();
    }

    function _contextSuffixLength() internal view override(ContextUpgradeable, ERC2771ContextUpgradeable) returns (uint256) {
        return ERC2771ContextUpgradeable._contextSuffixLength();
    }
}
