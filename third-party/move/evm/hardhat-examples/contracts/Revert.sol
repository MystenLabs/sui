//SPDX-License-Identifier: Unlicense
pragma solidity ^0.8.0;

contract Revert_Sol {
    function revertIf0(uint64 x) public pure returns (uint64) {
        if (x == 0) {
            revert();
        }
        return x;
    }

    function revertWithMessage() public pure returns (uint64) {
        revert('error message');
    }
}
