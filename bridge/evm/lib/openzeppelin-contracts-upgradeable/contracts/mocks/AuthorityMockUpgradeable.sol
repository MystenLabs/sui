// SPDX-License-Identifier: MIT

pragma solidity ^0.8.20;

import {IAccessManaged} from "@openzeppelin/contracts/access/manager/IAccessManaged.sol";
import {IAuthority} from "@openzeppelin/contracts/access/manager/IAuthority.sol";
import {Initializable} from "../proxy/utils/Initializable.sol";

contract NotAuthorityMockUpgradeable is Initializable, IAuthority {
    function __NotAuthorityMock_init() internal onlyInitializing {
    }

    function __NotAuthorityMock_init_unchained() internal onlyInitializing {
    }
    function canCall(address /* caller */, address /* target */, bytes4 /* selector */) external pure returns (bool) {
        revert("AuthorityNoDelayMock: not implemented");
    }
}

contract AuthorityNoDelayMockUpgradeable is Initializable, IAuthority {
    bool _immediate;

    function __AuthorityNoDelayMock_init() internal onlyInitializing {
    }

    function __AuthorityNoDelayMock_init_unchained() internal onlyInitializing {
    }
    function canCall(
        address /* caller */,
        address /* target */,
        bytes4 /* selector */
    ) external view returns (bool immediate) {
        return _immediate;
    }

    function _setImmediate(bool immediate) external {
        _immediate = immediate;
    }
}

contract AuthorityDelayMockUpgradeable is Initializable {
    bool _immediate;
    uint32 _delay;

    function __AuthorityDelayMock_init() internal onlyInitializing {
    }

    function __AuthorityDelayMock_init_unchained() internal onlyInitializing {
    }
    function canCall(
        address /* caller */,
        address /* target */,
        bytes4 /* selector */
    ) external view returns (bool immediate, uint32 delay) {
        return (_immediate, _delay);
    }

    function _setImmediate(bool immediate) external {
        _immediate = immediate;
    }

    function _setDelay(uint32 delay) external {
        _delay = delay;
    }
}

contract AuthorityNoResponseUpgradeable is Initializable {
    function __AuthorityNoResponse_init() internal onlyInitializing {
    }

    function __AuthorityNoResponse_init_unchained() internal onlyInitializing {
    }
    function canCall(address /* caller */, address /* target */, bytes4 /* selector */) external view {}
}

contract AuthoritiyObserveIsConsumingUpgradeable is Initializable {
    event ConsumeScheduledOpCalled(address caller, bytes data, bytes4 isConsuming);

    function __AuthoritiyObserveIsConsuming_init() internal onlyInitializing {
    }

    function __AuthoritiyObserveIsConsuming_init_unchained() internal onlyInitializing {
    }
    function canCall(
        address /* caller */,
        address /* target */,
        bytes4 /* selector */
    ) external pure returns (bool immediate, uint32 delay) {
        return (false, 1);
    }

    function consumeScheduledOp(address caller, bytes memory data) public {
        emit ConsumeScheduledOpCalled(caller, data, IAccessManaged(msg.sender).isConsumingScheduledOp());
    }
}
