// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

// These contracts are for testing only, they are not safe for use in production.

contract WithConstructor {
    /// @custom:oz-upgrades-unsafe-allow state-variable-immutable
    uint256 public immutable a;

    uint256 public b;

    /// @custom:oz-upgrades-unsafe-allow constructor
    constructor(uint256 _a) {
        a = _a;
    }

    function initialize(uint256 _b) public {
        b = _b;
    }
}

contract NoInitializer {
    /// @custom:oz-upgrades-unsafe-allow state-variable-immutable
    uint256 public immutable a;

    /// @custom:oz-upgrades-unsafe-allow constructor
    constructor(uint256 _a) {
        a = _a;
    }
}
