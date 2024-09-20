// SPDX-License-Identifier: MIT

pragma solidity ^0.8.20;

import {ERC20VotesUpgradeable} from "../../token/ERC20/extensions/ERC20VotesUpgradeable.sol";
import {ERC721VotesUpgradeable} from "../../token/ERC721/extensions/ERC721VotesUpgradeable.sol";
import {SafeCast} from "@openzeppelin/contracts/utils/math/SafeCast.sol";
import {Initializable} from "../../proxy/utils/Initializable.sol";

abstract contract ERC20VotesTimestampMockUpgradeable is Initializable, ERC20VotesUpgradeable {
    function __ERC20VotesTimestampMock_init() internal onlyInitializing {
    }

    function __ERC20VotesTimestampMock_init_unchained() internal onlyInitializing {
    }
    function clock() public view virtual override returns (uint48) {
        return SafeCast.toUint48(block.timestamp);
    }

    // solhint-disable-next-line func-name-mixedcase
    function CLOCK_MODE() public view virtual override returns (string memory) {
        return "mode=timestamp";
    }
}

abstract contract ERC721VotesTimestampMockUpgradeable is Initializable, ERC721VotesUpgradeable {
    function __ERC721VotesTimestampMock_init() internal onlyInitializing {
    }

    function __ERC721VotesTimestampMock_init_unchained() internal onlyInitializing {
    }
    function clock() public view virtual override returns (uint48) {
        return SafeCast.toUint48(block.timestamp);
    }

    // solhint-disable-next-line func-name-mixedcase
    function CLOCK_MODE() public view virtual override returns (string memory) {
        return "mode=timestamp";
    }
}
