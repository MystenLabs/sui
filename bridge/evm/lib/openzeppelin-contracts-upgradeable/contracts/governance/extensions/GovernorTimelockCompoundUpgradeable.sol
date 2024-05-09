// SPDX-License-Identifier: MIT
// OpenZeppelin Contracts (last updated v5.0.0) (governance/extensions/GovernorTimelockCompound.sol)

pragma solidity ^0.8.20;

import {IGovernor} from "@openzeppelin/contracts/governance/IGovernor.sol";
import {GovernorUpgradeable} from "../GovernorUpgradeable.sol";
import {ICompoundTimelock} from "@openzeppelin/contracts/vendor/compound/ICompoundTimelock.sol";
import {Address} from "@openzeppelin/contracts/utils/Address.sol";
import {SafeCast} from "@openzeppelin/contracts/utils/math/SafeCast.sol";
import {Initializable} from "../../proxy/utils/Initializable.sol";

/**
 * @dev Extension of {Governor} that binds the execution process to a Compound Timelock. This adds a delay, enforced by
 * the external timelock to all successful proposal (in addition to the voting duration). The {Governor} needs to be
 * the admin of the timelock for any operation to be performed. A public, unrestricted,
 * {GovernorTimelockCompound-__acceptAdmin} is available to accept ownership of the timelock.
 *
 * Using this model means the proposal will be operated by the {TimelockController} and not by the {Governor}. Thus,
 * the assets and permissions must be attached to the {TimelockController}. Any asset sent to the {Governor} will be
 * inaccessible.
 */
abstract contract GovernorTimelockCompoundUpgradeable is Initializable, GovernorUpgradeable {
    /// @custom:storage-location erc7201:openzeppelin.storage.GovernorTimelockCompound
    struct GovernorTimelockCompoundStorage {
        ICompoundTimelock _timelock;
    }

    // keccak256(abi.encode(uint256(keccak256("openzeppelin.storage.GovernorTimelockCompound")) - 1)) & ~bytes32(uint256(0xff))
    bytes32 private constant GovernorTimelockCompoundStorageLocation = 0x7d1501d734d0ca30b8d26751a7fae89646767b24afe11265192d56e5fe515b00;

    function _getGovernorTimelockCompoundStorage() private pure returns (GovernorTimelockCompoundStorage storage $) {
        assembly {
            $.slot := GovernorTimelockCompoundStorageLocation
        }
    }

    /**
     * @dev Emitted when the timelock controller used for proposal execution is modified.
     */
    event TimelockChange(address oldTimelock, address newTimelock);

    /**
     * @dev Set the timelock.
     */
    function __GovernorTimelockCompound_init(ICompoundTimelock timelockAddress) internal onlyInitializing {
        __GovernorTimelockCompound_init_unchained(timelockAddress);
    }

    function __GovernorTimelockCompound_init_unchained(ICompoundTimelock timelockAddress) internal onlyInitializing {
        _updateTimelock(timelockAddress);
    }

    /**
     * @dev Overridden version of the {Governor-state} function with added support for the `Expired` state.
     */
    function state(uint256 proposalId) public view virtual override returns (ProposalState) {
        GovernorTimelockCompoundStorage storage $ = _getGovernorTimelockCompoundStorage();
        ProposalState currentState = super.state(proposalId);

        return
            (currentState == ProposalState.Queued &&
                block.timestamp >= proposalEta(proposalId) + $._timelock.GRACE_PERIOD())
                ? ProposalState.Expired
                : currentState;
    }

    /**
     * @dev Public accessor to check the address of the timelock
     */
    function timelock() public view virtual returns (address) {
        GovernorTimelockCompoundStorage storage $ = _getGovernorTimelockCompoundStorage();
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
        bytes32 /*descriptionHash*/
    ) internal virtual override returns (uint48) {
        GovernorTimelockCompoundStorage storage $ = _getGovernorTimelockCompoundStorage();
        uint48 etaSeconds = SafeCast.toUint48(block.timestamp + $._timelock.delay());

        for (uint256 i = 0; i < targets.length; ++i) {
            if (
                $._timelock.queuedTransactions(keccak256(abi.encode(targets[i], values[i], "", calldatas[i], etaSeconds)))
            ) {
                revert GovernorAlreadyQueuedProposal(proposalId);
            }
            $._timelock.queueTransaction(targets[i], values[i], "", calldatas[i], etaSeconds);
        }

        return etaSeconds;
    }

    /**
     * @dev Overridden version of the {Governor-_executeOperations} function that run the already queued proposal
     * through the timelock.
     */
    function _executeOperations(
        uint256 proposalId,
        address[] memory targets,
        uint256[] memory values,
        bytes[] memory calldatas,
        bytes32 /*descriptionHash*/
    ) internal virtual override {
        GovernorTimelockCompoundStorage storage $ = _getGovernorTimelockCompoundStorage();
        uint256 etaSeconds = proposalEta(proposalId);
        if (etaSeconds == 0) {
            revert GovernorNotQueuedProposal(proposalId);
        }
        Address.sendValue(payable($._timelock), msg.value);
        for (uint256 i = 0; i < targets.length; ++i) {
            $._timelock.executeTransaction(targets[i], values[i], "", calldatas[i], etaSeconds);
        }
    }

    /**
     * @dev Overridden version of the {Governor-_cancel} function to cancel the timelocked proposal if it has already
     * been queued.
     */
    function _cancel(
        address[] memory targets,
        uint256[] memory values,
        bytes[] memory calldatas,
        bytes32 descriptionHash
    ) internal virtual override returns (uint256) {
        GovernorTimelockCompoundStorage storage $ = _getGovernorTimelockCompoundStorage();
        uint256 proposalId = super._cancel(targets, values, calldatas, descriptionHash);

        uint256 etaSeconds = proposalEta(proposalId);
        if (etaSeconds > 0) {
            // do external call later
            for (uint256 i = 0; i < targets.length; ++i) {
                $._timelock.cancelTransaction(targets[i], values[i], "", calldatas[i], etaSeconds);
            }
        }

        return proposalId;
    }

    /**
     * @dev Address through which the governor executes action. In this case, the timelock.
     */
    function _executor() internal view virtual override returns (address) {
        GovernorTimelockCompoundStorage storage $ = _getGovernorTimelockCompoundStorage();
        return address($._timelock);
    }

    /**
     * @dev Accept admin right over the timelock.
     */
    // solhint-disable-next-line private-vars-leading-underscore
    function __acceptAdmin() public {
        GovernorTimelockCompoundStorage storage $ = _getGovernorTimelockCompoundStorage();
        $._timelock.acceptAdmin();
    }

    /**
     * @dev Public endpoint to update the underlying timelock instance. Restricted to the timelock itself, so updates
     * must be proposed, scheduled, and executed through governance proposals.
     *
     * For security reasons, the timelock must be handed over to another admin before setting up a new one. The two
     * operations (hand over the timelock) and do the update can be batched in a single proposal.
     *
     * Note that if the timelock admin has been handed over in a previous operation, we refuse updates made through the
     * timelock if admin of the timelock has already been accepted and the operation is executed outside the scope of
     * governance.

     * CAUTION: It is not recommended to change the timelock while there are other queued governance proposals.
     */
    function updateTimelock(ICompoundTimelock newTimelock) external virtual onlyGovernance {
        _updateTimelock(newTimelock);
    }

    function _updateTimelock(ICompoundTimelock newTimelock) private {
        GovernorTimelockCompoundStorage storage $ = _getGovernorTimelockCompoundStorage();
        emit TimelockChange(address($._timelock), address(newTimelock));
        $._timelock = newTimelock;
    }
}
