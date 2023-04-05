//SPDX-License-Identifier: Unlicense
pragma solidity ^0.8.0;

contract StructABI_Sol {

    address private c;

    struct S {
        uint64 a;
        bool b;
        S2 c;
    }

    struct S2 {
        uint64 x;
    }

    constructor(address _c) {
        c = _c;
    }

    function call_s() public returns (bool) {
        S memory s = S(42, true, S2(41));
        (bool b, ) = c.call(abi.encodeWithSignature("test((uint64,bool,(uint64)))", s));
        return b;
    }

    function safeTransferFrom(S memory s) public returns (S2 memory) {
        return s.c;
    }

}
