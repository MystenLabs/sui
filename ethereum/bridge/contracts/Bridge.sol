// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.20;

import '@openzeppelin/contracts/token/ERC20/IERC20.sol';
import '@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol';
import '@openzeppelin/contracts/utils/cryptography/ECDSA.sol';
import '@openzeppelin/contracts/utils/ReentrancyGuard.sol';
import '@openzeppelin/contracts/utils/cryptography/MessageHashUtils.sol';
import '@openzeppelin/contracts-upgradeable/proxy/utils/Initializable.sol';
import '@openzeppelin/contracts-upgradeable/proxy/utils/UUPSUpgradeable.sol';
import {ERC721Upgradeable} from '@openzeppelin/contracts-upgradeable/token/ERC721/ERC721Upgradeable.sol';

import './interfaces/IBridge.sol';

// Bridge contract
contract Bridge is Initializable, UUPSUpgradeable, ERC721Upgradeable, IBridge {
	using SafeERC20 for IERC20;
	using MessageHashUtils for bytes32;

	uint256[48] __gap;

	mapping(address => mapping(uint => bool)) public processedNonces;

	// uint8 private immutable version;
	// uint8 private version;

	bool public paused;

	uint16 public constant MAX_TOTAL_WEIGHT = 10000;
	uint256 public constant MAX_SINGLE_VALIDATOR_WEIGHT = 1000;
	uint256 public constant APPROVAL_THRESHOLD = 3333;

	// A mapping from address to validator index
	mapping(address => uint256) public validatorIndex;

	// An array to store the validators
	Validator[] public validators;

	// Mapping of user address to nonce
	mapping(address => uint256) public nonces;

	// Function to pause the bridge
	function pauseBridge() private isRunning {
		paused = true;
	}

	// Function to pause the bridge
	function resumeBridge() private isPaused {
		paused = false;
	}

	// modifier to check if bridge is running
	modifier isRunning() {
		// If the first argument of 'require' evaluates to 'false', execution terminates and all
		// changes to the state and to Ether balances are reverted.
		// This used to consume all gas in old EVM versions, but not anymore.
		// It is often a good idea to use 'require' to check if functions are called correctly.
		// As a second argument, you can also provide an explanation about what went wrong.
		require(!paused, 'Bridge is not Running');
		_;
	}

	// modifier to check if bridge is paused
	modifier isPaused() {
		// If the first argument of 'require' evaluates to 'false', execution terminates and all
		// changes to the state and to Ether balances are reverted.
		// This used to consume all gas in old EVM versions, but not anymore.
		// It is often a good idea to use 'require' to check if functions are called correctly.
		// As a second argument, you can also provide an explanation about what went wrong.
		require(paused, 'Bridge is Paused');
		_;
	}

	function initialize(Validator[] calldata _validators) public initializer {
		// addValidator(firstPK, firstWeight);
		// __Ownable_init();
		__UUPSUpgradeable_init();
		paused = false;

		for (uint256 i = 0; i < _validators.length; i++) {
			addValidator(_validators[i].addr, _validators[i].weight);
			emit ValidatorAdded(_validators[i].addr, _validators[i].weight);
		}
	}

	function approveBridgeMessage(
		BridgeMessage calldata bridgeMessage,
		bytes[] calldata signatures
	) public isRunning returns (bool, uint256) {
		uint256 totalWeight = 0;
		// verify signatures
		bytes32 hash = ethSignedMessageHash(bridgeMessage);
		for (uint256 i = 0; i < signatures.length; i++) {
			address recoveredPK = recoverSigner(hash, signatures[i]);
			// Check if the address is not zero
			require(recoveredPK != address(0), 'Invalid signature: Recovered Zero address.');
			uint256 index = validatorIndex[recoveredPK] - 1;
			require(index < validators.length, 'Index out of bounds');

			Validator memory validator = validators[index];
			require(recoveredPK == validator.addr, 'Invalid signature');
			totalWeight += validator.weight;
		}

		if (bridgeMessage.messageType == 1) pauseBridge();

		return (true, totalWeight);
	}

	function resumePausedBridge(
		BridgeMessage calldata bridgeMessage,
		bytes[] calldata signatures
	) public isPaused {
		uint256 totalWeight = 0;
		// verify signatures
		bytes32 hash = ethSignedMessageHash(bridgeMessage);
		for (uint256 i = 0; i < signatures.length; i++) {
			address recoveredPK = recoverSigner(hash, signatures[i]);
			// Check if the address is not zero
			require(recoveredPK != address(0), 'Invalid signature: Recovered Zero address.');
			uint256 index = validatorIndex[recoveredPK] - 1;
			require(index < validators.length, 'Index out of bounds');

			Validator memory validator = validators[index];
			require(recoveredPK == validator.addr, 'Invalid signature');
			totalWeight += validator.weight;
		}

		// TODO
		require(totalWeight >= 999, 'Not enough total signature weight');
		if (bridgeMessage.messageType == 1) resumeBridge();
	}

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

	function validatorsCount() public view returns (uint count) {
		return validators.length;
	}

	// The contract can be upgraded by the owner
	function _authorizeUpgrade(address newImplementation) internal override {}

	// Function to initiate a transfer from the source chain to the destination chain
	function initiateTransfer(address recipient, uint256 amount) external {
		// Transfer the tokens from the sender to this contract
		// require(
		//     IERC20(token).transferFrom(msg.sender, address(this), amount),
		//     "Transfer failed"
		// );
	}

	// Function to complete a transfer from the destination chain to the source chain
	function completeTransfer(
		address sender,
		address recipient,
		uint256 amount,
		uint256 nonce,
		bytes memory signature
	) private {
		// Verify that the nonce is correct
		require(processedNonces[recipient][nonce] == false, 'transfer already processed');

		// Increment the nonce for the recipient
		processedNonces[recipient][nonce] = true;

		// Emit the transfer completed event
		emit TransferCompleted(sender, recipient, amount, nonce);
	}

	// returning the contract's balance in wei
	function getBalance() public view returns (uint256) {
		return address(this).balance;
	}

	function transfer(address payable transferAddress, uint256 amount) public {
		transferAddress.transfer(amount);
	}

	// "0x93f82d7903c6a37336c33d68a890b448665735b4f513003cb4ef0029da0372b9329e0f6fc0b9f9c0c77d66bbf7217da260803fcae345a72f7a7764c56f464b5c1b"
	// [1 ,2 ,3 ,4 ,"0x5567f54B29B973343d632f7BFCe9507343D41FCa" ,5 ,"0x5567f54B29B973343d632f7BFCe9507343D41FCa"]
	function ethSignedMessageHash(
		BridgeMessage calldata bridgeMessage
	) public pure returns (bytes32) {
		bytes32 hash = keccak256(
			abi.encodePacked(
				bridgeMessage.messageType,
				bridgeMessage.version,
				bridgeMessage.sourceChain,
				bridgeMessage.bridgeSeqNum,
				bridgeMessage.senderAddress,
				bridgeMessage.targetChain,
				bridgeMessage.targetAddress
			)
		);
		return MessageHashUtils.toEthSignedMessageHash(hash);
	}

	function ethSignedMessageHash(string memory message) public pure returns (bytes32) {
		bytes32 hash = keccak256(abi.encodePacked(message));
		return MessageHashUtils.toEthSignedMessageHash(hash);
	}

	function recoverSigner(bytes32 hash, bytes calldata signature) public pure returns (address) {
		return ECDSA.recover(hash, signature);
	}
}
