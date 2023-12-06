// contracts/Getters.sol
// SPDX-License-Identifier: Apache 2

pragma solidity ^0.8.20;

import "./BridgeState.sol";

contract BridgeGetters is BridgeState {
    function evmChainId() public view returns (uint256) {
        return _state.evmChainId;
    }

    function isFork() public view returns (bool) {
        return evmChainId() != block.chainid;
    }
}
