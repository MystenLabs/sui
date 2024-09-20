// SPDX-License-Identifier: MIT
// OpenZeppelin Contracts (last updated v5.0.0) (governance/extensions/GovernorStorage.sol)

pragma solidity ^0.8.20;

import {GovernorUpgradeable} from "../GovernorUpgradeable.sol";
import {Initializable} from "../../proxy/utils/Initializable.sol";

/**
 * @dev Extension of {Governor} that implements storage of proposal details. This modules also provides primitives for
 * the enumerability of proposals.
 *
 * Use cases for this module include:
 * - UIs that explore the proposal state without relying on event indexing.
 * - Using only the proposalId as an argument in the {Governor-queue} and {Governor-execute} functions for L2 chains
 *   where storage is cheap compared to calldata.
 */
abstract contract GovernorStorageUpgradeable is Initializable, GovernorUpgradeable {
    struct ProposalDetails {
        address[] targets;
        uint256[] values;
        bytes[] calldatas;
        bytes32 descriptionHash;
    }

    /// @custom:storage-location erc7201:openzeppelin.storage.GovernorStorage
    struct GovernorStorageStorage {
        uint256[] _proposalIds;
        mapping(uint256 proposalId => ProposalDetails) _proposalDetails;
    }

    // keccak256(abi.encode(uint256(keccak256("openzeppelin.storage.GovernorStorage")) - 1)) & ~bytes32(uint256(0xff))
    bytes32 private constant GovernorStorageStorageLocation = 0x7fd223d3380145bd26132714391e777c488a0df7ac2dd4b66419d8549fb3a600;

    function _getGovernorStorageStorage() private pure returns (GovernorStorageStorage storage $) {
        assembly {
            $.slot := GovernorStorageStorageLocation
        }
    }

    function __GovernorStorage_init() internal onlyInitializing {
    }

    function __GovernorStorage_init_unchained() internal onlyInitializing {
    }
    /**
     * @dev Hook into the proposing mechanism
     */
    function _propose(
        address[] memory targets,
        uint256[] memory values,
        bytes[] memory calldatas,
        string memory description,
        address proposer
    ) internal virtual override returns (uint256) {
        GovernorStorageStorage storage $ = _getGovernorStorageStorage();
        uint256 proposalId = super._propose(targets, values, calldatas, description, proposer);

        // store
        $._proposalIds.push(proposalId);
        $._proposalDetails[proposalId] = ProposalDetails({
            targets: targets,
            values: values,
            calldatas: calldatas,
            descriptionHash: keccak256(bytes(description))
        });

        return proposalId;
    }

    /**
     * @dev Version of {IGovernorTimelock-queue} with only `proposalId` as an argument.
     */
    function queue(uint256 proposalId) public virtual {
        GovernorStorageStorage storage $ = _getGovernorStorageStorage();
        // here, using storage is more efficient than memory
        ProposalDetails storage details = $._proposalDetails[proposalId];
        queue(details.targets, details.values, details.calldatas, details.descriptionHash);
    }

    /**
     * @dev Version of {IGovernor-execute} with only `proposalId` as an argument.
     */
    function execute(uint256 proposalId) public payable virtual {
        GovernorStorageStorage storage $ = _getGovernorStorageStorage();
        // here, using storage is more efficient than memory
        ProposalDetails storage details = $._proposalDetails[proposalId];
        execute(details.targets, details.values, details.calldatas, details.descriptionHash);
    }

    /**
     * @dev ProposalId version of {IGovernor-cancel}.
     */
    function cancel(uint256 proposalId) public virtual {
        GovernorStorageStorage storage $ = _getGovernorStorageStorage();
        // here, using storage is more efficient than memory
        ProposalDetails storage details = $._proposalDetails[proposalId];
        cancel(details.targets, details.values, details.calldatas, details.descriptionHash);
    }

    /**
     * @dev Returns the number of stored proposals.
     */
    function proposalCount() public view virtual returns (uint256) {
        GovernorStorageStorage storage $ = _getGovernorStorageStorage();
        return $._proposalIds.length;
    }

    /**
     * @dev Returns the details of a proposalId. Reverts if `proposalId` is not a known proposal.
     */
    function proposalDetails(
        uint256 proposalId
    ) public view virtual returns (address[] memory, uint256[] memory, bytes[] memory, bytes32) {
        GovernorStorageStorage storage $ = _getGovernorStorageStorage();
        // here, using memory is more efficient than storage
        ProposalDetails memory details = $._proposalDetails[proposalId];
        if (details.descriptionHash == 0) {
            revert GovernorNonexistentProposal(proposalId);
        }
        return (details.targets, details.values, details.calldatas, details.descriptionHash);
    }

    /**
     * @dev Returns the details (including the proposalId) of a proposal given its sequential index.
     */
    function proposalDetailsAt(
        uint256 index
    ) public view virtual returns (uint256, address[] memory, uint256[] memory, bytes[] memory, bytes32) {
        GovernorStorageStorage storage $ = _getGovernorStorageStorage();
        uint256 proposalId = $._proposalIds[index];
        (
            address[] memory targets,
            uint256[] memory values,
            bytes[] memory calldatas,
            bytes32 descriptionHash
        ) = proposalDetails(proposalId);
        return (proposalId, targets, values, calldatas, descriptionHash);
    }
}
