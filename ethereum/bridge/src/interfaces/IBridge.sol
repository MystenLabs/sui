// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.20;

import './IEnums.sol';

interface IBridge is IEnums {
	// STRUCTS

	// struct Erc20Transfer {
	// 	bytes32 dataDigest;
	// 	uint256 amount;
	// 	address from;
	// 	address to;
	// }

	struct TokenBridgingMessage {
		MessageType messageType;
		uint8 messageVersion;
		uint64 nonce;
		ChainID sourceChain;
		uint8 sourceChainTxIdLength;
		bytes sourceChainTxId;
		uint8 sourceChainEventIndex;
		uint8 senderAddressLength;
		bytes senderAddress;
		ChainID targetChain;
		uint8 targetChainLength;
		bytes targetAddress;
		TokenID tokenType;
		uint64 amount;
	}

	struct CommitteeBlocklistMessage {
		MessageType messageType;
		uint8 messageVersion;
		uint64 nonce;
		BlockListType blocklistType;
	}

	struct EmergencyOpMessage {
		MessageType messageType;
		uint8 messageVersion;
		uint64 nonce;
		EmergencyOpType opType;
	}

	// Define a struct for BridgeMessage
	struct BridgeMessage {
		MessageType messageType;
		uint8 messageVersion;
		uint64 sequenceNumber;
		ChainID sourceChain;
		bytes payload;
	}

	// A struct to represent a validator
	struct Member {
		address account; // The address of the validator
		uint256 stake; // The weight of the validator
	}

	// Define a struct for ApprovedBridgeMessage
	struct ApprovedBridgeMessage {
		BridgeMessage message;
		uint64 approvedEpoch;
		bytes[] signatures;
	}

	// Define a struct for BridgeMessageKey
	struct BridgeMessageKey {
		ChainID sourceChain;
		uint64 bridgeSeqNum;
	}

	// Define a struct for TokenBridgePayload
	struct TokenBridgePayload {
		address senderAddress;
		ChainID targetChain;
		address targetAddress;
		TokenID tokenType;
		uint64 amount;
	}

	// Struct to store the transfer history of an address
	struct TransferHistory {
		uint256 transferTime; // The timestamp of the transfer
		uint256 amount; // The amount of tokens transferred
	}

	// EVENTS

	event CommitteeMemberAdded(
		address account, // The address of the validator
		uint256 stake // The weight of the validator
	);

	// Event to emit when a transfer is initiated
	event TransferInitiated(
		address indexed sender,
		address indexed recipient,
		uint256 amount,
		uint256 nonce
	);

	// Event to emit when a transfer is completed
	event TransferCompleted(
		address indexed sender,
		address indexed recipient,
		uint256 amount,
		uint256 nonce
	);

	event ContractUpgraded(address indexed oldContract, address indexed newContract);

	event TransferRedeemed(
		uint16 indexed emitterChainId,
		bytes32 indexed emitterAddress,
		uint64 indexed sequence
	);

	event BridgeEvent(BridgeMessage message);
}
