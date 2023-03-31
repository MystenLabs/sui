//SPDX-License-Identifier: Unlicense
pragma solidity ^0.8.0;

contract Callee {

function success_uint() public pure returns (uint) {
    return 42;
}

function panic() public pure returns (uint) {
    //assert(false);
    uint i = 0;
    uint j = 1;
    return j/i;
}

function ret_revert() public pure returns (uint) {
    revert("revert");
    return 1;
}

}
