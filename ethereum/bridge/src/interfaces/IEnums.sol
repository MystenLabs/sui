// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.20;

interface IEnums {
	// Define an enum for the chain IDs
	enum ChainID {
		SUI_MAINNET,
		SUI_TESTNET,
		SUI_DEVNET,
		ETH_MAINNET,
		ETH_SEPOLIA
	}

	enum EmergencyOpType {
		FREEZE,
		UNFREEZE
	}

	enum MessageType {
		TOKEN,
		COMMITTEE_BLOCKLIST,
		EMERGENCY_OP
	}

	enum TokenID {
		SUI,
		BTC,
		ETH,
		USDC,
		USDT
	}

	enum BlockListType {
		BLOCKLIST,
		UNBLOCKLIST
	}
}
