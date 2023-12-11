// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.20;

interface IMessageType {
	// Define an enum for the Message Types
	enum MessageType {
		TOKEN,
		COMMITTEE_BLOCKLIST,
		EMERGENCY_OP
	}
}
