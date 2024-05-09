// SPDX-License-Identifier: MIT

pragma solidity ^0.8.20;

import {AccessManagedUpgradeable} from "../access/manager/AccessManagedUpgradeable.sol";
import {StorageSlot} from "@openzeppelin/contracts/utils/StorageSlot.sol";
import {Initializable} from "../proxy/utils/Initializable.sol";

abstract contract AccessManagedTargetUpgradeable is Initializable, AccessManagedUpgradeable {
    event CalledRestricted(address caller);
    event CalledUnrestricted(address caller);
    event CalledFallback(address caller);

    function __AccessManagedTarget_init() internal onlyInitializing {
    }

    function __AccessManagedTarget_init_unchained() internal onlyInitializing {
    }
    function fnRestricted() public restricted {
        emit CalledRestricted(msg.sender);
    }

    function fnUnrestricted() public {
        emit CalledUnrestricted(msg.sender);
    }

    function setIsConsumingScheduledOp(bool isConsuming, bytes32 slot) external {
        // Memory layout is 0x....<_consumingSchedule (boolean)><authority (address)>
        bytes32 mask = bytes32(uint256(1 << 160));
        if (isConsuming) {
            StorageSlot.getBytes32Slot(slot).value |= mask;
        } else {
            StorageSlot.getBytes32Slot(slot).value &= ~mask;
        }
    }

    fallback() external {
        emit CalledFallback(msg.sender);
    }
}
