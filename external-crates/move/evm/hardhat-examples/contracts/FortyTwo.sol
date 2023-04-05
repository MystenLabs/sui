//SPDX-License-Identifier: Unlicense
pragma solidity ^0.8.0;

contract FortyTwo_Sol {
    function forty_two() public pure returns (uint64) {
        return 42;
    }

    function forty_two_as_u256() public pure returns (uint256) {
        return 42;
    }

    function forty_two_as_string() public pure returns (string memory) {
        return "forty two";
    }

    function forty_two_plus_alpha(uint64 alpha) public pure returns (uint64) {
        return 42 + alpha;
    }
}
