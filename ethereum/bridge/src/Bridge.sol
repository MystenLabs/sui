// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.20;

import '@openzeppelin/contracts/token/ERC20/IERC20.sol';
import '@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol';
import '@openzeppelin/contracts/utils/ReentrancyGuard.sol';
import '@openzeppelin/contracts/utils/cryptography/ECDSA.sol';
import '@openzeppelin/contracts/utils/cryptography/MessageHashUtils.sol';
// import '@openzeppelin-upgradeable/contracts-upgradeable/proxy/utils/UUPSUpgradeable.sol';
// import {ERC721Upgradeable} from '@openzeppelin-upgradeable/contracts-upgradeable/token/ERC721/ERC721Upgradeable.sol';

import './interfaces/IBridge.sol';

// Bridge contract
contract Bridge is IBridge {
	using SafeERC20 for IERC20;
	using MessageHashUtils for bytes32;

	uint64 public validatorsCount;

	uint256[48] __gap;

	mapping(uint256 => bool) public processedNonces;

	// Define a mapping to store the token balances
	mapping(TokenID => uint256) public tokenBalances;
	// Define a mapping to store the token contracts
	mapping(address => IERC20) public tokenContracts;

	uint64 public version;
	uint8 public messageVersion;

	bool public running;

	uint16 public constant MAX_TOTAL_STAKE = 10000; // 100%
	uint256 public constant MAX_SINGLE_COMMITEE_MEMBER_STAKE = 1000; // 10%
	uint256 public constant APPROVAL_THRESHOLD = 3333;
	// Maximum amount of tokens that can be transferred per transaction
	uint256 public constant MAX_TRANSFER_AMOUNT = 1000;
	// Maximum amount of tokens that can be transferred by an address per day
	uint256 public constant MAX_DAILY_TRANSFER_AMOUNT = 10000;

	// SUI chain IDs
	uint8 public constant SUI_MAINNET = 0;
	uint8 public constant SUI_TESTNET = 1;
	uint8 public constant SUI_DEVNET = 2;

	// Ethereum chain IDs
	uint8 public constant ETH_MAINNET = 10;
	uint8 public constant ETH_SEPOLIA = 11;

	// A mapping from address to validator
	mapping(address => Member) public committee;

	mapping(MessageType => uint64) public sequenceNumbers;
	uint8[] private messageTypesInSequenceNumbersMapping;

	// Array to store the frozen signers
	address[] private bridgeFrozenSigners;

	// Mapping to store the transfer history
	mapping(uint256 => uint256[]) public transferHistory;

	// Declare the USDT and USDC token contracts
	IERC20 usdt;
	IERC20 usdc;

	mapping(bytes32 => address) public tokens;

	// Modifier to check the daily transfer limit
	modifier checkDailyLimit(uint256 transferTime, uint256 _amount) {
		// Calculate the sum of transfers during the past 24 hours
		uint256 sum = 0;
		for (uint256 i = 0; i < transferHistory[transferTime].length; i++) {
			uint256 historyAmount = transferHistory[transferTime][i];
			// Only consider transfers that happened within the past 24 hours
			if (block.timestamp - transferTime <= 24 hours) {
				sum += historyAmount;
			}
		}
		// Require that the sum of transfers plus the current amount is less than or equal to the daily limit
		require(sum + _amount <= MAX_DAILY_TRANSFER_AMOUNT, 'Daily transfer limit exceeded');
		_;
	}

	// Modifier to check the maximum transfer amount
	modifier checkMaxAmount(uint256 _amount) {
		// Require that the amount is less than or equal to the maximum amount
		require(_amount <= MAX_TRANSFER_AMOUNT, 'Maximum transfer amount exceeded');
		_;
	}

	// Define a modifier to check the message version
	modifier validMessageVersion(BridgeMessage calldata bridgeMessage) {
		require(bridgeMessage.messageVersion == messageVersion, 'Invalid message version');
		_;
	}

	// Define a modifier to check the message version
	modifier validTokenBridgingMessageVersion(TokenBridgingMessage calldata tokenBridgingMessage) {
		require(tokenBridgingMessage.messageVersion == messageVersion, 'Invalid message version');
		_;
	}

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

	// the modifier to check if the caller is a validator
	modifier onlyCommittee() {
		require(committee[msg.sender].stake > 0, 'Only committee memebers can call this function');
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

	function initialize(Member[] calldata _committeeMembers) public {
		// __UUPSUpgradeable_init();

		validatorsCount = 0;

		// TODO: Remove this
		// Declare the USDT and USDC contract addresses
		address USDT_ADDRESS = 0xdAC17F958D2ee523a2206206994597C13D831ec7; // Mainnet address
		address USDC_ADDRESS = 0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48; // Mainnet address

		// Declare the USDT and USDC token contracts
		usdt = IERC20(USDT_ADDRESS);
		usdc = IERC20(USDC_ADDRESS);

		uint256 totalStake = 0;
		for (uint256 i = 0; i < _committeeMembers.length; i++) {
			addCommitteeMember(_committeeMembers[i].account, _committeeMembers[i].stake);
			emit CommitteeMemberAdded(_committeeMembers[i].account, _committeeMembers[i].stake);
			totalStake += _committeeMembers[i].stake;
		}

		require(totalStake <= MAX_TOTAL_STAKE, 'Total stake is too high');
		running = true;
		version = 1;
		messageVersion = 1;
	}

	function executeDeposit(bytes memory data) public {
		address tokenAddress;
		address recipient;
		uint256 amount;

		(tokenAddress, recipient, amount) = abi.decode(data, (address, address, uint256));

		// Check the daily transfer limit for the recipient
		// checkDailyLimit(recipient, amount);

		// Check the maximum transfer amount
		// checkMaxAmount(amount);

		// Transfer the tokens from the handler to the recipient
		IERC20(tokenAddress).transferFrom(msg.sender, address(this), amount);

		// Record the transfer history for the recipient
		transferHistory[block.timestamp].push(amount);
	}

	function tokenBridging(
		TokenBridgingMessage calldata tokenBridgingMessage,
		bytes[] calldata signatures
	) external payable whenRunning validTokenBridgingMessageVersion(tokenBridgingMessage) {
		require(tokenBridgingMessage.messageType == MessageType.TOKEN, 'Invalid message type');

		// uint64 tokenBridgingSeqNum = nextSeqNum(tokenBridgingMessage.messageType);
		// require(
		// 	tokenBridgingMessage.sequenceNumber == tokenBridgingSeqNum,
		// 	'Invalid sequence number for the token bridging message'
		// );

		// Verify the signatures
		(address[] memory seen, uint256 totalStake) = verifyTokenBridgingSignatures(
			tokenBridgingMessage,
			signatures
		);

		if (tokenBridgingMessage.tokenType == TokenID.ETH) {
			// Require the amount of ETH sent to match the amount parameter
			require(msg.value == tokenBridgingMessage.amount, 'Incorrect ETH amount');
			tokenBalances[tokenBridgingMessage.tokenType] += uint256(tokenBridgingMessage.amount);
		} else if (tokenBridgingMessage.tokenType == TokenID.USDC) {
			// Transfer the USDC tokens from the caller to the contract
			// Require the caller to approve the contract to spend their tokens before calling this function
			usdc.transferFrom(msg.sender, address(this), tokenBridgingMessage.amount);
			tokenBalances[tokenBridgingMessage.tokenType] += uint256(tokenBridgingMessage.amount);
		} else if (tokenBridgingMessage.tokenType == TokenID.USDT) {
			// Transfer the USDT tokens from the caller to the contract
			// Require the caller to approve the contract to spend their tokens before calling this function
			usdt.transferFrom(msg.sender, address(this), tokenBridgingMessage.amount);
			tokenBalances[tokenBridgingMessage.tokenType] += uint256(tokenBridgingMessage.amount);
		} else {
			// Revert the transaction if the currency type is invalid
			revert('Invalid currency type');
		}
	}

	function verifyTokenBridgingSignatures(
		TokenBridgingMessage calldata tbm,
		bytes[] calldata signatures
	) public view returns (address[] memory, uint256) {
		// Declare an array to store the recovered addresses
		address[] memory seen = new address[](signatures.length);
		uint256 seenIndex = 0;
		uint256 totalStake = 0;

		bytes32 hash = keccak256(
			abi.encodePacked(
				'SUI_NATIVE_BRIDGE',
				tbm.messageType,
				tbm.messageVersion,
				tbm.nonce,
				tbm.sourceChain,
				tbm.sourceChainTxIdLength,
				tbm.sourceChainTxId,
				tbm.sourceChainEventIndex,
				tbm.senderAddressLength,
				tbm.senderAddress,
				tbm.targetChain,
				tbm.targetChainLength,
				tbm.targetAddress,
				tbm.tokenType,
				tbm.amount
			)
		);
		bytes32 signedMessageHash = MessageHashUtils.toEthSignedMessageHash(hash);

		// Verify Signatures
		for (uint256 i = 0; i < signatures.length; i++) {
			address recoveredPK = recoverSigner(signedMessageHash, signatures[i]);

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

		return (seen, totalStake);
	}

	function freezeBridge(
		BridgeMessage calldata bridgeMessage,
		bytes[] calldata signatures
	) public whenRunning {
		require(bridgeMessage.messageVersion == messageVersion, 'Invalid message version');
		require(bridgeMessage.messageType == MessageType.EMERGENCY_OP, 'Invalid message type');
		uint64 emergencyOpSeqNum = nextSeqNum(bridgeMessage.messageType);
		require(
			bridgeMessage.sequenceNumber == emergencyOpSeqNum,
			'Invalid sequence number for the emergency operation'
		);
		(address[] memory seen, ) = verifySignatures(bridgeMessage, signatures);
		require(seen.length >= 2, 'Not enough signatures to approve the emergency operation');
		pauseBridge();
		bridgeFrozenSigners = seen;
	}

	function unfreezeBridge(
		BridgeMessage calldata bridgeMessage,
		bytes[] calldata signatures
	) public whenNotRunning {
		require(bridgeMessage.messageVersion == messageVersion, 'Invalid message version');
		require(bridgeMessage.messageType == MessageType.EMERGENCY_OP, 'Invalid message type');
		uint64 emergencyOpSeqNum = nextSeqNum(bridgeMessage.messageType);
		require(
			bridgeMessage.sequenceNumber == emergencyOpSeqNum,
			'Invalid sequence number for the emergency operation'
		);
		(, uint256 totalStake) = verifySignatures(bridgeMessage, signatures);
		require(totalStake >= 5100, 'Not enough signatures to approve the emergency operation');
		resumeBridge();
		bridgeFrozenSigners = new address[](0);
	}

	function verifySignatures(
		BridgeMessage calldata bridgeMessage,
		bytes[] calldata signatures
	) private view returns (address[] memory, uint256) {
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

		return (seen, totalStake);
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

	function addCommitteeMember(address _pk, uint256 _stake) private {
		// Check if the address is not zero
		require(_pk != address(0), 'Zero address.');
		// Check if the stake is not zero
		require(_stake != 0, 'Zero stake.');
		// Check if the stake is not too high
		require(_stake <= MAX_SINGLE_COMMITEE_MEMBER_STAKE, 'Stake is too high');
		// Check if the address is not already a validator
		require(committee[_pk].account == address(0), 'Already a Committee Member.');

		// Add the validator to the mapping
		committee[_pk] = Member(_pk, _stake);
		++validatorsCount;
	}

	// The contract can be upgraded by the owner
	// function _authorizeUpgrade(address newImplementation) internal override {}

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
		require(processedNonces[nonce] == false, 'transfer already processed');

		// Increment the nonce for the recipient
		processedNonces[nonce] = true;

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

	function ethSignedMessageHash(
		BridgeMessage calldata bridgeMessage
	) public pure returns (bytes32) {
		bytes32 hash = keccak256(
			abi.encodePacked(
				bridgeMessage.messageType,
				bridgeMessage.messageVersion,
				bridgeMessage.sequenceNumber,
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

	function nextSeqNum(MessageType msgType) private returns (uint64) {
		// Check if the message type is already in the mapping
		for (uint256 i = 0; i < messageTypesInSequenceNumbersMapping.length; i++) {
			if (messageTypesInSequenceNumbersMapping[i] == uint8(msgType)) {
				// Get the sequence number for the message type
				uint64 seqNum = sequenceNumbers[msgType];
				// Increment the sequence number by 1 and update the mapping
				sequenceNumbers[msgType] = seqNum + 1;
				// Return the original sequence number
				return seqNum;
			}
		}
		// Set the sequence number for the message type to 0
		sequenceNumbers[msgType] = 0;
		// Add the message type to the mapping
		messageTypesInSequenceNumbersMapping.push(uint8(msgType));
		// Return 0
		return 0;
	}

	event BridgeEvent(address from, address to, uint256 amount);

	function initBridgingTokenTx(address to) public payable {
		emit BridgeEvent(msg.sender, to, msg.value);
	}
}
