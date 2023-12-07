// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.20;

interface IBridge {
	// ENUMS

	// Define an enum for the chain IDs
	enum ChainID {
		SUI_CHAIN,
		BTC_CHAIN,
		ETH_CHAIN,
		TMP_CHAIN
	}

	// Define an enum for the token IDs
	enum TokenID {
		SUI_TOKEN,
		BTC_TOKEN,
		ETH_TOKEN,
		USDC_TOKEN,
		USDT_TOKEN
	}

	// STRUCTS

	struct Erc20Transfer {
		bytes32 dataDigest;
		uint256 amount;
		address from;
		address to;
	}

	struct BridgeMessage {
		// 0: token , 1: object ? TBD
		uint8 messageType;
		uint8 version;
		ChainID sourceChain;
		uint64 bridgeSeqNum;
		address senderAddress;
		uint8 targetChain;
		address targetAddress;
		// bytes payload;
	}

	// A struct to represent a validator
	struct Validator {
		address addr; // The address of the validator
		uint256 weight; // The weight of the validator
	}

	struct ApprovedBridgeMessage {
		BridgeMessage message;
		uint64 approvedEpoch;
		bytes[] signatures;
	}

	struct BridgeMessageKey {
		uint8 sourceChain;
		uint64 bridgeSeqNum;
	}

	// EVENTS

	// Event to emit when a transfer is initiated
	event ValidatorAdded(
		address addr, // The address of the validator
		uint256 weight // The weight of the validator
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

	// FUNCTIONS

	/**
	// function _parseTransferCommon(bytes memory encoded) external pure returns (Transfer memory transfer);

    function attestToken(address tokenAddress, uint32 nonce) external payable returns (uint64 sequence);

    function wrapAndTransferETH(uint16 recipientChain, bytes32 recipient, uint256 arbiterFee, uint32 nonce) external payable returns (uint64 sequence);

    function wrapAndTransferETHWithPayload(uint16 recipientChain, bytes32 recipient, uint32 nonce, bytes memory payload) external payable returns (uint64 sequence);

    function transferTokens(address token, uint256 amount, uint16 recipientChain, bytes32 recipient, uint256 arbiterFee, uint32 nonce) external payable returns (uint64 sequence);

    function transferTokensWithPayload(address token, uint256 amount, uint16 recipientChain, bytes32 recipient, uint32 nonce, bytes memory payload) external payable returns (uint64 sequence);

    function updateWrapped(bytes memory encodedVm) external returns (address token);

    function createWrapped(bytes memory encodedVm) external returns (address token);

    function completeTransferWithPayload(bytes memory encodedVm) external returns (bytes memory);

    function completeTransferAndUnwrapETHWithPayload(bytes memory encodedVm) external returns (bytes memory);

    function completeTransfer(bytes memory encodedVm) external;

    function completeTransferAndUnwrapETH(bytes memory encodedVm) external;

    // function encodeAssetMeta(AssetMeta memory meta) external pure returns (bytes memory encoded);

    // function encodeTransfer(Transfer memory transfer) external pure returns (bytes memory encoded);

    // function encodeTransferWithPayload(TransferWithPayload memory transfer) external pure returns (bytes memory encoded);

    function parsePayloadID(bytes memory encoded) external pure returns (uint8 payloadID);

    // function parseAssetMeta(bytes memory encoded) external pure returns (AssetMeta memory meta);

    // function parseTransfer(bytes memory encoded) external pure returns (Transfer memory transfer);

    // function parseTransferWithPayload(bytes memory encoded) external pure returns (TransferWithPayload memory transfer);

    function governanceActionIsConsumed(bytes32 hash) external view returns (bool);

    function isInitialized(address impl) external view returns (bool);

    function isTransferCompleted(bytes32 hash) external view returns (bool);
	 
	 */
}
