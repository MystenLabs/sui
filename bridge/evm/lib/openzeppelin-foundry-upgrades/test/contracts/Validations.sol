// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

// These contracts are for testing only, they are not safe for use in production.

contract Unsafe {
    function unsafe() public {
        (bool s, ) = msg.sender.delegatecall("");
        s;
    }
}

contract LayoutV1 {
    uint256 a;
    uint256 b;
}

contract LayoutV2_Bad {
    uint256 a;
    uint256 c;
    uint256 b;
}

/// @custom:oz-upgrades-from LayoutV1
contract LayoutV2_Renamed {
    uint256 _old_a;
    uint256 b;
}

/// @custom:oz-upgrades-from LayoutV1
contract LayoutV2_UpgradesFrom_Bad {
    uint256 a;
    uint256 c;
    uint256 b;
}

contract NamespacedV1 {
    /// @custom:storage-location erc7201:s
    struct S {
        uint256 a;
        uint256 b;
    }
}

contract NamespacedV2_Bad {
    /// @custom:storage-location erc7201:s
    struct S {
        uint256 a;
        uint256 c;
        uint256 b;
    }
}

/// @custom:oz-upgrades-from NamespacedV1
contract NamespacedV2_UpgradesFrom_Bad {
    /// @custom:storage-location erc7201:s
    struct S {
        uint256 a;
        uint256 c;
        uint256 b;
    }
}

contract NamespacedV2_Ok {
    /// @custom:storage-location erc7201:s
    struct S {
        uint256 a;
        uint256 b;
        uint256 c;
    }
}

/// @custom:oz-upgrades-from NamespacedV1
contract NamespacedV2_UpgradesFrom_Ok {
    /// @custom:storage-location erc7201:s
    struct S {
        uint256 a;
        uint256 b;
        uint256 c;
    }
}
