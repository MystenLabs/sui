// SPDX-License-Identifier: MIT
// OpenZeppelin Contracts (last updated v5.0.0) (governance/extensions/GovernorVotesQuorumFraction.sol)

pragma solidity ^0.8.20;

import {GovernorVotesUpgradeable} from "./GovernorVotesUpgradeable.sol";
import {SafeCast} from "@openzeppelin/contracts/utils/math/SafeCast.sol";
import {Checkpoints} from "@openzeppelin/contracts/utils/structs/Checkpoints.sol";
import {Initializable} from "../../proxy/utils/Initializable.sol";

/**
 * @dev Extension of {Governor} for voting weight extraction from an {ERC20Votes} token and a quorum expressed as a
 * fraction of the total supply.
 */
abstract contract GovernorVotesQuorumFractionUpgradeable is Initializable, GovernorVotesUpgradeable {
    using Checkpoints for Checkpoints.Trace208;

    /// @custom:storage-location erc7201:openzeppelin.storage.GovernorVotesQuorumFraction
    struct GovernorVotesQuorumFractionStorage {
        Checkpoints.Trace208 _quorumNumeratorHistory;
    }

    // keccak256(abi.encode(uint256(keccak256("openzeppelin.storage.GovernorVotesQuorumFraction")) - 1)) & ~bytes32(uint256(0xff))
    bytes32 private constant GovernorVotesQuorumFractionStorageLocation = 0xe770710421fd2cad75ad828c61aa98f2d77d423a440b67872d0f65554148e000;

    function _getGovernorVotesQuorumFractionStorage() private pure returns (GovernorVotesQuorumFractionStorage storage $) {
        assembly {
            $.slot := GovernorVotesQuorumFractionStorageLocation
        }
    }

    event QuorumNumeratorUpdated(uint256 oldQuorumNumerator, uint256 newQuorumNumerator);

    /**
     * @dev The quorum set is not a valid fraction.
     */
    error GovernorInvalidQuorumFraction(uint256 quorumNumerator, uint256 quorumDenominator);

    /**
     * @dev Initialize quorum as a fraction of the token's total supply.
     *
     * The fraction is specified as `numerator / denominator`. By default the denominator is 100, so quorum is
     * specified as a percent: a numerator of 10 corresponds to quorum being 10% of total supply. The denominator can be
     * customized by overriding {quorumDenominator}.
     */
    function __GovernorVotesQuorumFraction_init(uint256 quorumNumeratorValue) internal onlyInitializing {
        __GovernorVotesQuorumFraction_init_unchained(quorumNumeratorValue);
    }

    function __GovernorVotesQuorumFraction_init_unchained(uint256 quorumNumeratorValue) internal onlyInitializing {
        _updateQuorumNumerator(quorumNumeratorValue);
    }

    /**
     * @dev Returns the current quorum numerator. See {quorumDenominator}.
     */
    function quorumNumerator() public view virtual returns (uint256) {
        GovernorVotesQuorumFractionStorage storage $ = _getGovernorVotesQuorumFractionStorage();
        return $._quorumNumeratorHistory.latest();
    }

    /**
     * @dev Returns the quorum numerator at a specific timepoint. See {quorumDenominator}.
     */
    function quorumNumerator(uint256 timepoint) public view virtual returns (uint256) {
        GovernorVotesQuorumFractionStorage storage $ = _getGovernorVotesQuorumFractionStorage();
        uint256 length = $._quorumNumeratorHistory._checkpoints.length;

        // Optimistic search, check the latest checkpoint
        Checkpoints.Checkpoint208 storage latest = $._quorumNumeratorHistory._checkpoints[length - 1];
        uint48 latestKey = latest._key;
        uint208 latestValue = latest._value;
        if (latestKey <= timepoint) {
            return latestValue;
        }

        // Otherwise, do the binary search
        return $._quorumNumeratorHistory.upperLookupRecent(SafeCast.toUint48(timepoint));
    }

    /**
     * @dev Returns the quorum denominator. Defaults to 100, but may be overridden.
     */
    function quorumDenominator() public view virtual returns (uint256) {
        return 100;
    }

    /**
     * @dev Returns the quorum for a timepoint, in terms of number of votes: `supply * numerator / denominator`.
     */
    function quorum(uint256 timepoint) public view virtual override returns (uint256) {
        return (token().getPastTotalSupply(timepoint) * quorumNumerator(timepoint)) / quorumDenominator();
    }

    /**
     * @dev Changes the quorum numerator.
     *
     * Emits a {QuorumNumeratorUpdated} event.
     *
     * Requirements:
     *
     * - Must be called through a governance proposal.
     * - New numerator must be smaller or equal to the denominator.
     */
    function updateQuorumNumerator(uint256 newQuorumNumerator) external virtual onlyGovernance {
        _updateQuorumNumerator(newQuorumNumerator);
    }

    /**
     * @dev Changes the quorum numerator.
     *
     * Emits a {QuorumNumeratorUpdated} event.
     *
     * Requirements:
     *
     * - New numerator must be smaller or equal to the denominator.
     */
    function _updateQuorumNumerator(uint256 newQuorumNumerator) internal virtual {
        GovernorVotesQuorumFractionStorage storage $ = _getGovernorVotesQuorumFractionStorage();
        uint256 denominator = quorumDenominator();
        if (newQuorumNumerator > denominator) {
            revert GovernorInvalidQuorumFraction(newQuorumNumerator, denominator);
        }

        uint256 oldQuorumNumerator = quorumNumerator();
        $._quorumNumeratorHistory.push(clock(), SafeCast.toUint208(newQuorumNumerator));

        emit QuorumNumeratorUpdated(oldQuorumNumerator, newQuorumNumerator);
    }
}
