// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.20;

interface ITokenID {
	// Define an enum for the token IDs
	enum TokenID {
		SUI,
		BTC,
		ETH,
		USDC,
		USDT
	}
}
