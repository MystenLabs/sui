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

	uint64 public validatorsCount = 0;

	uint256[48] __gap;

	mapping(address => mapping(uint => bool)) public processedNonces;

	uint64 public version;
	// nonce for replay protection
	uint64 public sequenceNumber;
	// // committee pub keys
	// committee: BridgeCommittee,
	// // Escrow for storing native tokens
	// escrow: BridgeEscrow,
	// Bridge treasury for mint/burn bridged tokens
	// treasury: BridgeTreasury,

	// Use a mapping from bytes32 to BridgeMessage
	mapping(bytes32 => BridgeMessage) public pendingMessages;
	// Use a mapping from bytes32 to ApprovedBridgeMessage
	mapping(bytes32 => ApprovedBridgeMessage) public approvedMessages;

	bool public running;
	uint64 public lastEmergencyOpSeqNum;

	uint16 public constant MAX_TOTAL_WEIGHT = 10000;
	uint256 public constant MAX_SINGLE_VALIDATOR_STAKE = 1000;
	uint256 public constant APPROVAL_THRESHOLD = 3333;

	// A mapping from address to validator
	mapping(address => Member) public committee;

	// Mapping of user address to nonce
	mapping(address => uint256) public nonces;

	/**
	 * @dev Modifier to make a function callable only when the contract is Running.
	 */
	modifier whenRunning() {
		require(running, 'Bridge is Running');
		_;
	}

	/**
	 * @dev Modifier to make a function callable only when the contract is not Running.
	 */
	modifier whenNotRunning() {
		require(!running, 'Bridge is Not Running');
		_;
	}

	// Function to pause the bridge
	function pauseBridge() private whenRunning {
		running = false;
	}

	// Function to pause the bridge
	function resumeBridge() private whenNotRunning {
		running = true;
	}

	function initialize(Member[] calldata _committeeMembers) public initializer {
		// addValidator(firstPK, firstWeight);
		// __Ownable_init();
		__UUPSUpgradeable_init();
		running = true;

		for (uint256 i = 0; i < _committeeMembers.length; i++) {
			addCommitteeMember(_committeeMembers[i].account, _committeeMembers[i].stake);
			emit CommitteeMemberAdded(_committeeMembers[i].account, _committeeMembers[i].stake);
		}
	}

	function approveBridgeMessage(
		BridgeMessage calldata bridgeMessage,
		bytes[] calldata signatures
	) public whenRunning returns (bool, uint256) {
		// Declare an array to store the recovered addresses
		address[] memory seen = new address[](signatures.length);
		uint256 seenIndex = 0;

		uint256 totalStake = 0;
		bytes32 hash = ethSignedMessageHash(bridgeMessage);

		// Verify Signatures
		for (uint256 i = 0; i < signatures.length; i++) {
			address recoveredPK = recoverSigner(hash, signatures[i]);

			// Check if the address is not zero
			require(recoveredPK != address(0), 'Invalid signature: Recovered Zero address.');

			// Check if the address has already been seen
			bool found = false;
			for (uint256 j = 0; j < seen.length; j++) {
				if (seen[j] == recoveredPK) {
					found = true;
					break;
				}
			}
			require(!found, 'Duplicate signature: Address already seen');

			// Add the address to the array
			seen[seenIndex++] = recoveredPK;

			// Retrieve the Validator directly from the mapping
			Member memory member = committee[recoveredPK];

			// Validate the recovered address
			require(member.account != address(0), 'Invalid Signer, not a committee authority');
			require(recoveredPK == member.account, 'Invalid signature: Address mismatch');

			totalStake += member.stake;
		}

		// // retrieve pending message if source chain is Ethereum
		// if (bridgeMessage.sourceChain == ChainID.ETH) {
		// 	BridgeMessageKey memory key = BridgeMessageKey(
		// 		bridgeMessage.sourceChain,
		// 		bridgeMessage.targetChain,
		// 		bridgeMessage.targetAddress,
		// 		bridgeMessage.tokenAddress,
		// 		bridgeMessage.tokenId
		// 	);

		// }

		if (bridgeMessage.messageType == MessageType.EMERGENCY_OP) pauseBridge();

		return (true, totalStake);
	}

	function resumePausedBridge(
		BridgeMessage calldata bridgeMessage,
		bytes[] calldata signatures
	) public whenNotRunning {
		uint256 totalStake = 0;
		bytes32 hash = ethSignedMessageHash(bridgeMessage);

		for (uint256 i = 0; i < signatures.length; i++) {
			address recoveredPK = recoverSigner(hash, signatures[i]);

			// Check if the address is not zero
			require(recoveredPK != address(0), 'Invalid signature: Recovered Zero address.');

			// Retrieve the Validator directly from the mapping
			Member memory member = committee[recoveredPK];

			// Validate the recovered address
			require(member.account != address(0), 'Invalid Signer, not a committee authority');
			require(recoveredPK == member.account, 'Invalid signature: Address mismatch');

			totalStake += member.stake;
		}

		// TODO: Add your desired total weight requirement
		require(totalStake >= 999, 'Not enough stake to resume the bridge');

		if (bridgeMessage.messageType == MessageType.EMERGENCY_OP) resumeBridge();
	}

	// Check also weight. i.e. no more than 33% of the total weight
	// A function to add a validator
	function addCommitteeMember(address _pk, uint256 _stake) private {
		// Check if the address is not zero
		require(_pk != address(0), 'Zero address.');

		// Check if the address is not already a validator
		require(committee[_pk].account == address(0), 'Already a Committee Member.');

		// Add the validator to the mapping
		committee[_pk] = Member(_pk, _stake);
		++validatorsCount;
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
				bridgeMessage.messageVersion,
				bridgeMessage.seqNum,
				bridgeMessage.sourceChain,
				bridgeMessage.payload
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

	// Define a function to set a pending message
	function setPendingMessage(BridgeMessageKey memory key, BridgeMessage memory value) external {
		// Generate a hash of the key values
		bytes32 hash = keccak256(abi.encode(key));
		// Store the value in the mapping
		pendingMessages[hash] = value;
	}

	// Define a function to get a pending message
	function getPendingMessage(
		BridgeMessageKey memory key
	) external view returns (BridgeMessage memory) {
		// Generate a hash of the key values
		bytes32 hash = keccak256(abi.encode(key));
		// Return the value from the mapping
		return pendingMessages[hash];
	}

	// Define a function to set an approved message
	function setApprovedMessage(
		BridgeMessageKey memory key,
		ApprovedBridgeMessage memory value
	) external {
		// Generate a hash of the key values
		bytes32 hash = keccak256(abi.encode(key));
		// Store the value in the mapping
		approvedMessages[hash] = value;
	}

	// Define a function to get an approved message
	function getApprovedMessage(
		BridgeMessageKey memory key
	) external view returns (ApprovedBridgeMessage memory) {
		// Generate a hash of the key values
		bytes32 hash = keccak256(abi.encode(key));
		// Return the value from the mapping
		return approvedMessages[hash];
	}
}
