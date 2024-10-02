// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "../../contracts/SuiBridge.sol";

contract MockSuiBridgeV2 is SuiBridge {
    uint8 public mock;
    bool public isPausing;

    function initializeV2() external {
        _pause();
    }

    function initializeV2Params(uint256 value, bool _override, string memory _event) external {
        if (_override) {
            _pause();
        } else if (value == 42) {
            _pause();
        }

        emit MockEvent(_event);
    }

    function newMockFunction(bool _pausing) external {
        isPausing = _pausing;
    }

    function newMockFunction(bool _pausing, uint8 _mock) external {
        mock = _mock;
        isPausing = _pausing;
    }

    // used to ignore for forge coverage
    function testSkip() external view {}

    event MockEvent(string _event);
}
