// SPDX-License-Identifier: MIT

pragma solidity ^0.8.20;

import {GovernorUpgradeable} from "../../governance/GovernorUpgradeable.sol";
import {GovernorCountingSimpleUpgradeable} from "../../governance/extensions/GovernorCountingSimpleUpgradeable.sol";
import {GovernorVotesUpgradeable} from "../../governance/extensions/GovernorVotesUpgradeable.sol";
import {Initializable} from "../../proxy/utils/Initializable.sol";

abstract contract GovernorWithParamsMockUpgradeable is Initializable, GovernorVotesUpgradeable, GovernorCountingSimpleUpgradeable {
    event CountParams(uint256 uintParam, string strParam);

    function __GovernorWithParamsMock_init() internal onlyInitializing {
    }

    function __GovernorWithParamsMock_init_unchained() internal onlyInitializing {
    }
    function quorum(uint256) public pure override returns (uint256) {
        return 0;
    }

    function votingDelay() public pure override returns (uint256) {
        return 4;
    }

    function votingPeriod() public pure override returns (uint256) {
        return 16;
    }

    function _getVotes(
        address account,
        uint256 blockNumber,
        bytes memory params
    ) internal view override(GovernorUpgradeable, GovernorVotesUpgradeable) returns (uint256) {
        uint256 reduction = 0;
        // If the user provides parameters, we reduce the voting weight by the amount of the integer param
        if (params.length > 0) {
            (reduction, ) = abi.decode(params, (uint256, string));
        }
        // reverts on overflow
        return super._getVotes(account, blockNumber, params) - reduction;
    }

    function _countVote(
        uint256 proposalId,
        address account,
        uint8 support,
        uint256 weight,
        bytes memory params
    ) internal override(GovernorUpgradeable, GovernorCountingSimpleUpgradeable) {
        if (params.length > 0) {
            (uint256 _uintParam, string memory _strParam) = abi.decode(params, (uint256, string));
            emit CountParams(_uintParam, _strParam);
        }
        return super._countVote(proposalId, account, support, weight, params);
    }
}
