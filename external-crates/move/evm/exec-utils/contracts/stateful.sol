// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

pragma solidity ^0.8.10;

contract TwoFunctions {
    uint counter = 0x0;

    function inc() public {
        counter = counter + 1;
    }

    function get() public view returns(uint) {
        return counter;
    }
}
