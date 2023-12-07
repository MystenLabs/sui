// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

contract SuiToEthBridge {
	function getVersion() public pure returns (uint256) {
		return 1;
	}

	// The address of the implementation contract
	address public implementation;

	struct Validator {
		address addr; // The address of the validator
		uint256 weight; // The weight of the validator
	}
	// The list of validators who can vote on upgrades
	Validator[] public validators;
	// A mapping from address to validator index
	mapping(address => uint256) public validatorIndex;

	// The struct that represents an upgrade proposal
	struct Proposal {
		address newImplementation; // The address of the new implementation contract
		uint256 nonce; // A nonce to prevent replay attacks
	}

	// The mapping that stores the signatures of the validators who have voted on a proposal
	mapping(bytes32 => bytes[]) public signatures;

	// The event that is emitted when an upgrade is executed
	event Upgraded(address oldImplementation, address newImplementation);

	// The constructor that sets the initial implementation and validators
	constructor(address _implementation, Validator[] memory _validators) {
		implementation = _implementation;
		for (uint256 i = 0; i < _validators.length; i++) {
			addValidator(_validators[i].addr, _validators[i].weight);
			emit ValidatorAdded(_validators[i].addr, _validators[i].weight);
		}
	}

	event ValidatorAdded(address addr, uint256 weight);

	// Check also weight. i.e. no more than 33% of the total weight
	// A function to add a validator
	function addValidator(address _pk, uint256 _weight) private {
		// Check if the address is not zero
		require(_pk != address(0), 'Zero address.');
		// Check if the address is not already a validator
		require(validatorIndex[_pk] == 0, 'Already a validator.');
		// Add the validator to the array
		validators.push(Validator(_pk, _weight));
		// Update the validator index
		validatorIndex[_pk] = validators.length;
	}

	// The modifier that delegates all calls to the implementation contract
	modifier delegated() {
		(bool success, bytes memory data) = implementation.delegatecall(msg.data);
		require(success, 'Delegatecall failed');
		assembly {
			return(add(data, 0x20), mload(data))
		}
		_;
	}

	// The fallback function that delegates all calls to the implementation contract
	fallback() external payable delegated {}

	// The function that allows anyone to submit an upgrade proposal, along with their signature
	function submitProposal(Proposal memory proposal, bytes memory signature) public {
		// Check that the sender is a validator
		require(isValidator(msg.sender), 'Sender is not a validator');
		// Check that the proposal is valid
		require(proposal.newImplementation != address(0), 'Invalid implementation address');
		require(proposal.nonce > 0, 'Invalid nonce');
		// Check that the signature is valid
		require(verifySignature(proposal, signature, msg.sender), 'Invalid signature');
		// Compute the proposal hash
		bytes32 proposalHash = getProposalHash(proposal);
		// Check that the proposal does not exist
		require(signatures[proposalHash].length == 0, 'Proposal already exists');
		// Store the signature
		signatures[proposalHash].push(signature);
	}

	// The function that allows anyone to submit additional signatures for an existing proposal
	function submitSignature(Proposal memory proposal, bytes memory signature) public {
		// Check that the sender is a validator
		require(isValidator(msg.sender), 'Sender is not a validator');
		// Check that the signature is valid
		require(verifySignature(proposal, signature, msg.sender), 'Invalid signature');
		// Compute the proposal hash
		bytes32 proposalHash = getProposalHash(proposal);
		// Check that the proposal exists
		require(signatures[proposalHash].length > 0, 'Proposal does not exist');
		// Check that the sender has not already signed the proposal
		require(!hasSigned(proposal, msg.sender), 'Sender has already signed');
		// Store the signature
		signatures[proposalHash].push(signature);
	}

	// The function that allows anyone to execute an upgrade, given a proposal and a sufficient number of signatures
	function executeUpgrade(Proposal memory proposal, bytes[] memory _signatures) public {
		// Check that the proposal has not expired
		require(block.timestamp < proposal.nonce + 1 days, 'Proposal has expired');
		// Check that the signatures are valid and match the proposal
		require(verifySignatures(proposal, _signatures), 'Invalid signatures');
		// Check that at least 2/3 of the validators have signed the proposal
		require(_signatures.length * 3 > validators.length * 2, 'Not enough signatures');
		// Store the old implementation address
		address oldImplementation = implementation;
		// Update the implementation address
		implementation = proposal.newImplementation;
		// Emit the upgraded event
		emit Upgraded(oldImplementation, implementation);
	}

	// The function that checks if an address is a validator
	function isValidator(address account) public view returns (bool) {
		for (uint256 i = 0; i < validators.length; i++) {
			if (validators[i].addr == account) {
				return true;
			}
		}
		return false;
	}

	// The function that computes the hash of a proposal
	function getProposalHash(Proposal memory proposal) public pure returns (bytes32) {
		return keccak256(abi.encode(proposal.newImplementation, proposal.nonce));
	}

	// The function that verifies a signature for a proposal
	function verifySignature(
		Proposal memory proposal,
		bytes memory signature,
		address signer
	) public pure returns (bool) {
		bytes32 proposalHash = getProposalHash(proposal);
		bytes32 messageHash = keccak256(
			abi.encodePacked('\x19Ethereum Signed Message:\n32', proposalHash)
		);
		address recovered = recover(messageHash, signature);
		return recovered == signer;
	}

	// The function that verifies multiple signatures for a proposal
	function verifySignatures(
		Proposal memory proposal,
		bytes[] memory _signatures
	) public view returns (bool) {
		bytes32 proposalHash = getProposalHash(proposal);
		bytes32 messageHash = keccak256(
			abi.encodePacked('\x19Ethereum Signed Message:\n32', proposalHash)
		);
		for (uint256 i = 0; i < _signatures.length; i++) {
			address recovered = recover(messageHash, _signatures[i]);
			if (!isValidator(recovered)) {
				return false;
			}
		}
		return true;
	}

	// The function that checks if a validator has signed a proposal
	function hasSigned(Proposal memory proposal, address signer) public view returns (bool) {
		bytes32 proposalHash = getProposalHash(proposal);
		bytes32 messageHash = keccak256(
			abi.encodePacked('\x19Ethereum Signed Message:\n32', proposalHash)
		);
		for (uint256 i = 0; i < signatures[proposalHash].length; i++) {
			address recovered = recover(messageHash, signatures[proposalHash][i]);
			if (recovered == signer) {
				return true;
			}
		}
		return false;
	}

	// The function that recovers an address from a message hash and a signature
	function recover(bytes32 messageHash, bytes memory signature) public pure returns (address) {
		bytes32 r;
		bytes32 s;
		uint8 v;
		if (signature.length != 65) {
			return address(0);
		}
		assembly {
			r := mload(add(signature, 0x20))
			s := mload(add(signature, 0x40))
			v := byte(0, mload(add(signature, 0x60)))
		}
		if (v < 27) {
			v += 27;
		}
		if (v != 27 && v != 28) {
			return address(0);
		}
		return ecrecover(messageHash, v, r, s);
	}
}
