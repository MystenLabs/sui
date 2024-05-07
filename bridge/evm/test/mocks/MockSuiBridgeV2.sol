// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "../../contracts/SuiBridge.sol";

contract MockSuiBridgeV2 is SuiBridge {
    uint8 public mock;
    bool public isPausing;

    function initializeV2() external {
        _pause();
    }

    function newMockFunction(bool _pausing) external {
        isPausing = _pausing;
    }

    function newMockFunction(bool _pausing, uint8 _mock) external {
        mock = _mock;
        isPausing = _pausing;
    }

    // used to ignore for forge coverage
    function test() external view {}
}
