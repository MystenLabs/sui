// SPDX-License-Identifier: MIT

pragma solidity ^0.8.20;

import {GovernorUpgradeable} from "../../governance/GovernorUpgradeable.sol";
import {GovernorPreventLateQuorumUpgradeable} from "../../governance/extensions/GovernorPreventLateQuorumUpgradeable.sol";
import {GovernorSettingsUpgradeable} from "../../governance/extensions/GovernorSettingsUpgradeable.sol";
import {GovernorCountingSimpleUpgradeable} from "../../governance/extensions/GovernorCountingSimpleUpgradeable.sol";
import {GovernorVotesUpgradeable} from "../../governance/extensions/GovernorVotesUpgradeable.sol";
import {Initializable} from "../../proxy/utils/Initializable.sol";

abstract contract GovernorPreventLateQuorumMockUpgradeable is
    Initializable, GovernorSettingsUpgradeable,
    GovernorVotesUpgradeable,
    GovernorCountingSimpleUpgradeable,
    GovernorPreventLateQuorumUpgradeable
{
    uint256 private _quorum;

    function __GovernorPreventLateQuorumMock_init(uint256 quorum_) internal onlyInitializing {
        __GovernorPreventLateQuorumMock_init_unchained(quorum_);
    }

    function __GovernorPreventLateQuorumMock_init_unchained(uint256 quorum_) internal onlyInitializing {
        _quorum = quorum_;
    }

    function quorum(uint256) public view override returns (uint256) {
        return _quorum;
    }

    function proposalDeadline(
        uint256 proposalId
    ) public view override(GovernorUpgradeable, GovernorPreventLateQuorumUpgradeable) returns (uint256) {
        return super.proposalDeadline(proposalId);
    }

    function proposalThreshold() public view override(GovernorUpgradeable, GovernorSettingsUpgradeable) returns (uint256) {
        return super.proposalThreshold();
    }

    function _castVote(
        uint256 proposalId,
        address account,
        uint8 support,
        string memory reason,
        bytes memory params
    ) internal override(GovernorUpgradeable, GovernorPreventLateQuorumUpgradeable) returns (uint256) {
        return super._castVote(proposalId, account, support, reason, params);
    }
}
