// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.20;

import '../tokens/EthToken.sol'; // import your token contract

contract EthTokenTreasury is EthToken {
	// the mapping of accounts that can receive funds from the treasury
	mapping(address => bool) public recipients;

	// the mapping of spending limits for each recipient
	mapping(address => uint256) public limits;

	// the modifier to check if the recipient is valid
	modifier validRecipient(address recipient) {
		require(recipients[recipient], 'Invalid recipient');
		_;
	}

	// the event to log when a new recipient is added
	event RecipientAdded(address recipient, uint256 limit);

	// the event to log when a recipient is removed
	event RecipientRemoved(address recipient);

	// the event to log when a limit is updated
	event LimitUpdated(address recipient, uint256 limit);

	// the event to log when funds are transferred
	event FundsTransferred(address recipient, uint256 amount);

	// the function to add a new recipient with a spending limit
	function addRecipient(address recipient, uint256 limit) internal {
		require(recipient != address(0), 'Zero address');
		require(!recipients[recipient], 'Recipient already exists');
		recipients[recipient] = true; // add the recipient to the mapping
		limits[recipient] = limit; // set the limit for the recipient
		emit RecipientAdded(recipient, limit); // emit the event
	}

	// the function to remove an existing recipient
	function removeRecipient(address recipient) internal validRecipient(recipient) {
		recipients[recipient] = false; // remove the recipient from the mapping
		limits[recipient] = 0; // reset the limit for the recipient
		emit RecipientRemoved(recipient); // emit the event
	}

	// the function to update the limit for an existing recipient
	function updateLimit(address recipient, uint256 limit) internal validRecipient(recipient) {
		limits[recipient] = limit; // update the limit for the recipient
		emit LimitUpdated(recipient, limit); // emit the event
	}

	// the function to transfer funds from the treasury to a recipient
	function transferFunds(address recipient, uint256 amount) internal validRecipient(recipient) {
		require(amount <= limits[recipient], 'Amount exceeds limit');
		_transfer(address(this), recipient, amount); // transfer the tokens from the treasury to the recipient
		emit FundsTransferred(recipient, amount); // emit the event
	}
}
