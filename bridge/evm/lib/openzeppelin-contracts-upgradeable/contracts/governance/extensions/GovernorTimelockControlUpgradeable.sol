// SPDX-License-Identifier: MIT
// OpenZeppelin Contracts (last updated v5.0.0) (governance/extensions/GovernorTimelockControl.sol)

pragma solidity ^0.8.20;

import {IGovernor} from "@openzeppelin/contracts/governance/IGovernor.sol";
import {GovernorUpgradeable} from "../GovernorUpgradeable.sol";
import {TimelockControllerUpgradeable} from "../TimelockControllerUpgradeable.sol";
import {IERC165} from "@openzeppelin/contracts/utils/introspection/IERC165.sol";
import {SafeCast} from "@openzeppelin/contracts/utils/math/SafeCast.sol";
import {Initializable} from "../../proxy/utils/Initializable.sol";

/**
 * @dev Extension of {Governor} that binds the execution process to an instance of {TimelockController}. This adds a
 * delay, enforced by the {TimelockController} to all successful proposal (in addition to the voting duration). The
 * {Governor} needs the proposer (and ideally the executor) roles for the {Governor} to work properly.
 *
 * Using this model means the proposal will be operated by the {TimelockController} and not by the {Governor}. Thus,
 * the assets and permissions must be attached to the {TimelockController}. Any asset sent to the {Governor} will be
 * inaccessible from a proposal, unless executed via {Governor-relay}.
 *
 * WARNING: Setting up the TimelockController to have additional proposers or cancellers besides the governor is very
 * risky, as it grants them the ability to: 1) execute operations as the timelock, and thus possibly performing
 * operations or accessing funds that are expected to only be accessible through a vote, and 2) block governance
 * proposals that have been approved by the voters, effectively executing a Denial of Service attack.
 *
 * NOTE: `AccessManager` does not support scheduling more than one operation with the same target and calldata at
 * the same time. See {AccessManager-schedule} for a workaround.
 */
abstract contract GovernorTimelockControlUpgradeable is Initializable, GovernorUpgradeable {
    /// @custom:storage-location erc7201:openzeppelin.storage.GovernorTimelockControl
    struct GovernorTimelockControlStorage {
        TimelockControllerUpgradeable _timelock;
        mapping(uint256 proposalId => bytes32) _timelockIds;
    }

    // keccak256(abi.encode(uint256(keccak256("openzeppelin.storage.GovernorTimelockControl")) - 1)) & ~bytes32(uint256(0xff))
    bytes32 private constant GovernorTimelockControlStorageLocation = 0x0d5829787b8befdbc6044ef7457d8a95c2a04bc99235349f1a212c063e59d400;

    function _getGovernorTimelockControlStorage() private pure returns (GovernorTimelockControlStorage storage $) {
        assembly {
            $.slot := GovernorTimelockControlStorageLocation
        }
    }

    /**
     * @dev Emitted when the timelock controller used for proposal execution is modified.
     */
    event TimelockChange(address oldTimelock, address newTimelock);

    /**
     * @dev Set the timelock.
     */
    function __GovernorTimelockControl_init(TimelockControllerUpgradeable timelockAddress) internal onlyInitializing {
        __GovernorTimelockControl_init_unchained(timelockAddress);
    }

    function __GovernorTimelockControl_init_unchained(TimelockControllerUpgradeable timelockAddress) internal onlyInitializing {
        _updateTimelock(timelockAddress);
    }

    /**
     * @dev Overridden version of the {Governor-state} function that considers the status reported by the timelock.
     */
    function state(uint256 proposalId) public view virtual override returns (ProposalState) {
        GovernorTimelockControlStorage storage $ = _getGovernorTimelockControlStorage();
        ProposalState currentState = super.state(proposalId);

        if (currentState != ProposalState.Queued) {
            return currentState;
        }

        bytes32 queueid = $._timelockIds[proposalId];
        if ($._timelock.isOperationPending(queueid)) {
            return ProposalState.Queued;
        } else if ($._timelock.isOperationDone(queueid)) {
            // This can happen if the proposal is executed directly on the timelock.
            return ProposalState.Executed;
        } else {
            // This can happen if the proposal is canceled directly on the timelock.
            return ProposalState.Canceled;
        }
    }

    /**
     * @dev Public accessor to check the address of the timelock
     */
    function timelock() public view virtual returns (address) {
        GovernorTimelockControlStorage storage $ = _getGovernorTimelockControlStorage();
        return address($._timelock);
    }

    /**
     * @dev See {IGovernor-proposalNeedsQueuing}.
     */
    function proposalNeedsQueuing(uint256) public view virtual override returns (bool) {
        return true;
    }

    /**
     * @dev Function to queue a proposal to the timelock.
     */
    function _queueOperations(
        uint256 proposalId,
        address[] memory targets,
        uint256[] memory values,
        bytes[] memory calldatas,
        bytes32 descriptionHash
    ) internal virtual override returns (uint48) {
        GovernorTimelockControlStorage storage $ = _getGovernorTimelockControlStorage();
        uint256 delay = $._timelock.getMinDelay();

        bytes32 salt = _timelockSalt(descriptionHash);
        $._timelockIds[proposalId] = $._timelock.hashOperationBatch(targets, values, calldatas, 0, salt);
        $._timelock.scheduleBatch(targets, values, calldatas, 0, salt, delay);

        return SafeCast.toUint48(block.timestamp + delay);
    }

    /**
     * @dev Overridden version of the {Governor-_executeOperations} function that runs the already queued proposal
     * through the timelock.
     */
    function _executeOperations(
        uint256 proposalId,
        address[] memory targets,
        uint256[] memory values,
        bytes[] memory calldatas,
        bytes32 descriptionHash
    ) internal virtual override {
        GovernorTimelockControlStorage storage $ = _getGovernorTimelockControlStorage();
        // execute
        $._timelock.executeBatch{value: msg.value}(targets, values, calldatas, 0, _timelockSalt(descriptionHash));
        // cleanup for refund
        delete $._timelockIds[proposalId];
    }

    /**
     * @dev Overridden version of the {Governor-_cancel} function to cancel the timelocked proposal if it has already
     * been queued.
     */
    // This function can reenter through the external call to the timelock, but we assume the timelock is trusted and
    // well behaved (according to TimelockController) and this will not happen.
    // slither-disable-next-line reentrancy-no-eth
    function _cancel(
        address[] memory targets,
        uint256[] memory values,
        bytes[] memory calldatas,
        bytes32 descriptionHash
    ) internal virtual override returns (uint256) {
        GovernorTimelockControlStorage storage $ = _getGovernorTimelockControlStorage();
        uint256 proposalId = super._cancel(targets, values, calldatas, descriptionHash);

        bytes32 timelockId = $._timelockIds[proposalId];
        if (timelockId != 0) {
            // cancel
            $._timelock.cancel(timelockId);
            // cleanup
            delete $._timelockIds[proposalId];
        }

        return proposalId;
    }

    /**
     * @dev Address through which the governor executes action. In this case, the timelock.
     */
    function _executor() internal view virtual override returns (address) {
        GovernorTimelockControlStorage storage $ = _getGovernorTimelockControlStorage();
        return address($._timelock);
    }

    /**
     * @dev Public endpoint to update the underlying timelock instance. Restricted to the timelock itself, so updates
     * must be proposed, scheduled, and executed through governance proposals.
     *
     * CAUTION: It is not recommended to change the timelock while there are other queued governance proposals.
     */
    function updateTimelock(TimelockControllerUpgradeable newTimelock) external virtual onlyGovernance {
        _updateTimelock(newTimelock);
    }

    function _updateTimelock(TimelockControllerUpgradeable newTimelock) private {
        GovernorTimelockControlStorage storage $ = _getGovernorTimelockControlStorage();
        emit TimelockChange(address($._timelock), address(newTimelock));
        $._timelock = newTimelock;
    }

    /**
     * @dev Computes the {TimelockController} operation salt.
     *
     * It is computed with the governor address itself to avoid collisions across governor instances using the
     * same timelock.
     */
    function _timelockSalt(bytes32 descriptionHash) private view returns (bytes32) {
        return bytes20(address(this)) ^ descriptionHash;
    }
}
