// SPDX-License-Identifier: UNLICENSED
pragma solidity 0.8.20;

import 'ds-test/test.sol';
import '../Bridge.sol';
import '../interfaces/IBridge.sol';

contract BridgeTest is DSTest, IBridge {
	Bridge public bridge;

	Member[] public committeeMembers;

	function setUp() public {
		// Initialize some committee members
		committeeMembers.push(Member(0x2EBDe1Fe7f387c5fF0fD5C43A2C78d59CCf705c4, 1000));
		committeeMembers.push(Member(0x90e55615B26bD34f9b21AbF2D62D3DF48baf9793, 1000));
		committeeMembers.push(Member(0xf7F3764FF720094dF34104af27a0f994Abd7d441, 1000));
		committeeMembers.push(Member(0x2A6b1A7Fa61Cc281f7867cD0f7F6F36b64ebA031, 1000));
		committeeMembers.push(Member(0x5161f030a0271388a9BEE2544Aa77538CA38dAF1, 1000));
		committeeMembers.push(Member(0x89F6664D4D1E39E780bb5c97eca474CfB8766CbB, 1000));
		committeeMembers.push(Member(0x34052dDAAF7a01224Eb1330Aa6C36751a5D5233B, 1000));
		committeeMembers.push(Member(0x8f04B9707df15864201a9abBee3dc036553bFFee, 1000));
		committeeMembers.push(Member(0xD2c77D22735155D4877056972adaaDD1Dc5Dd020, 1000));
		committeeMembers.push(Member(0xb0993760373B5e13B689A7E18DaBa7D960bC9843, 1000));

		// Deploy the bridge contract
		bridge = new Bridge();
		bridge.initialize(committeeMembers);
	}

	function testInitialize() public {
		// Check the initial state of the bridge contract
		assertEq(bridge.validatorsCount(), 10);
		assertEq(bridge.version(), 1);
		assertEq(bridge.messageVersion(), 1);
		assertTrue(bridge.running());
	}
}
