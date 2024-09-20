// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Proxiable} from "./Proxiable.sol";

// These contracts are for testing only, they are not safe for use in production.

/// @custom:oz-upgrades-from GreeterProxiable
contract GreeterV2Proxiable is Proxiable {
    string public greeting;

    function initialize(string memory _greeting) public {
        greeting = _greeting;
    }

    function resetGreeting() public {
        greeting = "resetted";
    }
}
