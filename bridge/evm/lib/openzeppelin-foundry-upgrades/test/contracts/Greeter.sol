// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

// These contracts are for testing only, they are not safe for use in production.

contract Greeter {
    string public greeting;

    function initialize(string memory _greeting) public {
        greeting = _greeting;
    }
}
