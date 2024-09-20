// SPDX-License-Identifier: MIT

pragma solidity ^0.8.20;

import {ReentrancyGuardUpgradeable} from "../utils/ReentrancyGuardUpgradeable.sol";
import {ReentrancyAttackUpgradeable} from "./ReentrancyAttackUpgradeable.sol";
import {Initializable} from "../proxy/utils/Initializable.sol";

contract ReentrancyMockUpgradeable is Initializable, ReentrancyGuardUpgradeable {
    uint256 public counter;

    function __ReentrancyMock_init() internal onlyInitializing {
        __ReentrancyGuard_init_unchained();
        __ReentrancyMock_init_unchained();
    }

    function __ReentrancyMock_init_unchained() internal onlyInitializing {
        counter = 0;
    }

    function callback() external nonReentrant {
        _count();
    }

    function countLocalRecursive(uint256 n) public nonReentrant {
        if (n > 0) {
            _count();
            countLocalRecursive(n - 1);
        }
    }

    function countThisRecursive(uint256 n) public nonReentrant {
        if (n > 0) {
            _count();
            (bool success, ) = address(this).call(abi.encodeCall(this.countThisRecursive, (n - 1)));
            require(success, "ReentrancyMock: failed call");
        }
    }

    function countAndCall(ReentrancyAttackUpgradeable attacker) public nonReentrant {
        _count();
        attacker.callSender(abi.encodeCall(this.callback, ()));
    }

    function _count() private {
        counter += 1;
    }

    function guardedCheckEntered() public nonReentrant {
        require(_reentrancyGuardEntered());
    }

    function unguardedCheckNotEntered() public view {
        require(!_reentrancyGuardEntered());
    }
}
