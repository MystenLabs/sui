// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ERC1967Utils} from "@openzeppelin/contracts/proxy/ERC1967/ERC1967Utils.sol";
import "./mocks/MockSuiBridgeV2.sol";
import "../contracts/BridgeCommittee.sol";
import "../contracts/SuiBridge.sol";
import "./BridgeBaseTest.t.sol";

contract CommitteeUpgradeableTest is BridgeBaseTest {
    MockSuiBridgeV2 bridgeV2;
    uint8 _chainID = 12;

    // This function is called before each unit test
    function setUp() public {
        setUpBridgeTest();
        address[] memory _committeeMembers = new address[](5);
        uint16[] memory _stake = new uint16[](5);
        _committeeMembers[0] = committeeMemberA;
        _committeeMembers[1] = committeeMemberB;
        _committeeMembers[2] = committeeMemberC;
        _committeeMembers[3] = committeeMemberD;
        _committeeMembers[4] = committeeMemberE;
        _stake[0] = 1000;
        _stake[1] = 1000;
        _stake[2] = 1000;
        _stake[3] = 2002;
        _stake[4] = 4998;

        uint8[] memory _supportedDestinationChains = new uint8[](1);
        _supportedDestinationChains[0] = 0;

        // deploy bridge committee
        address _committee = Upgrades.deployUUPSProxy(
            "BridgeCommittee.sol",
            abi.encodeCall(
                BridgeCommittee.initialize, (_committeeMembers, _stake, minStakeRequired)
            ),
            opts
        );

        committee = BridgeCommittee(_committee);

        // deploy bridge config
        address _config = Upgrades.deployUUPSProxy(
            "BridgeConfig.sol",
            abi.encodeCall(
                BridgeConfig.initialize,
                (_committee, _chainID, supportedTokens, tokenPrices, tokenIds, suiDecimals, _supportedDestinationChains)
            ),
            opts
        );

        committee.initializeConfig(_config);

        // deploy sui bridge
        address _bridge = Upgrades.deployUUPSProxy(
            "SuiBridge.sol",
            abi.encodeCall(SuiBridge.initialize, (_committee, address(0), address(0))),
            opts
        );

        bridge = SuiBridge(_bridge);
        bridgeV2 = new MockSuiBridgeV2();
    }

    function testUpgradeWithSignaturesSuccess() public {
        bytes memory initializer = abi.encodeCall(MockSuiBridgeV2.initializeV2, ());
        bytes memory payload = abi.encode(address(bridge), address(bridgeV2), initializer);

        // Create upgrade message
        BridgeUtils.Message memory message = BridgeUtils.Message({
            messageType: BridgeUtils.UPGRADE,
            version: 1,
            nonce: 0,
            chainID: _chainID,
            payload: payload
        });
        bytes memory encodedMessage = BridgeUtils.encodeMessage(message);
        bytes32 messageHash = keccak256(encodedMessage);
        bytes[] memory signatures = new bytes[](4);
        signatures[0] = getSignature(messageHash, committeeMemberPkA);
        signatures[1] = getSignature(messageHash, committeeMemberPkB);
        signatures[2] = getSignature(messageHash, committeeMemberPkC);
        signatures[3] = getSignature(messageHash, committeeMemberPkD);
        assertFalse(bridge.paused());
        bridge.upgradeWithSignatures(signatures, message);
        assertTrue(bridge.paused());
        assertEq(Upgrades.getImplementationAddress(address(bridge)), address(bridgeV2));
    }

    function testUpgradeWithSignaturesInsufficientStakeAmount() public {
        // Create message
        bytes memory initializer = abi.encodeCall(MockSuiBridgeV2.initializeV2, ());
        bytes memory payload = abi.encode(address(bridge), address(bridgeV2), initializer);

        // Create upgrade message
        BridgeUtils.Message memory message = BridgeUtils.Message({
            messageType: BridgeUtils.UPGRADE,
            version: 1,
            nonce: 0,
            chainID: _chainID,
            payload: payload
        });
        bytes memory encodedMessage = BridgeUtils.encodeMessage(message);
        bytes32 messageHash = keccak256(encodedMessage);
        bytes[] memory signatures = new bytes[](2);
        signatures[0] = getSignature(messageHash, committeeMemberPkA);
        signatures[1] = getSignature(messageHash, committeeMemberPkB);
        vm.expectRevert(bytes("BridgeCommittee: Insufficient stake amount"));
        bridge.upgradeWithSignatures(signatures, message);
    }

    function testUpgradeWithSignaturesMessageDoesNotMatchType() public {
        BridgeUtils.Message memory message = BridgeUtils.Message({
            messageType: BridgeUtils.TOKEN_TRANSFER,
            version: 1,
            nonce: 0,
            chainID: _chainID,
            payload: abi.encode(0)
        });
        bytes memory encodedMessage = BridgeUtils.encodeMessage(message);
        bytes32 messageHash = keccak256(encodedMessage);
        bytes[] memory signatures = new bytes[](4);
        signatures[0] = getSignature(messageHash, committeeMemberPkA);
        signatures[1] = getSignature(messageHash, committeeMemberPkB);
        signatures[2] = getSignature(messageHash, committeeMemberPkC);
        signatures[3] = getSignature(messageHash, committeeMemberPkD);
        vm.expectRevert(bytes("MessageVerifier: message does not match type"));
        bridge.upgradeWithSignatures(signatures, message);
    }

    function testUpgradeWithSignaturesInvalidNonce() public {
        BridgeUtils.Message memory message = BridgeUtils.Message({
            messageType: BridgeUtils.UPGRADE,
            version: 1,
            nonce: 10,
            chainID: _chainID,
            payload: abi.encode(0)
        });
        bytes memory encodedMessage = BridgeUtils.encodeMessage(message);
        bytes32 messageHash = keccak256(encodedMessage);
        bytes[] memory signatures = new bytes[](4);
        signatures[0] = getSignature(messageHash, committeeMemberPkA);
        signatures[1] = getSignature(messageHash, committeeMemberPkB);
        signatures[2] = getSignature(messageHash, committeeMemberPkC);
        signatures[3] = getSignature(messageHash, committeeMemberPkD);
        vm.expectRevert(bytes("MessageVerifier: Invalid nonce"));
        bridge.upgradeWithSignatures(signatures, message);
    }

    function testUpgradeWithSignaturesERC1967UpgradeNewImplementationIsNotUUPS() public {
        bytes memory initializer = abi.encodeCall(MockSuiBridgeV2.initializeV2, ());
        bytes memory payload = abi.encode(address(bridge), address(this), initializer);

        BridgeUtils.Message memory message = BridgeUtils.Message({
            messageType: BridgeUtils.UPGRADE,
            version: 1,
            nonce: 0,
            chainID: _chainID,
            payload: payload
        });
        bytes memory encodedMessage = BridgeUtils.encodeMessage(message);
        bytes32 messageHash = keccak256(encodedMessage);
        bytes[] memory signatures = new bytes[](4);
        signatures[0] = getSignature(messageHash, committeeMemberPkA);
        signatures[1] = getSignature(messageHash, committeeMemberPkB);
        signatures[2] = getSignature(messageHash, committeeMemberPkC);
        signatures[3] = getSignature(messageHash, committeeMemberPkD);
        assertFalse(bridge.paused());
        vm.expectRevert(
            abi.encodeWithSelector(
                ERC1967Utils.ERC1967InvalidImplementation.selector, address(this)
            )
        );
        bridge.upgradeWithSignatures(signatures, message);
    }

    function testUpgradeWithSignaturesInvalidProxyAddress() public {
        bytes memory initializer = abi.encodeCall(MockSuiBridgeV2.initializeV2, ());
        bytes memory payload = abi.encode(address(this), address(bridgeV2), initializer);

        BridgeUtils.Message memory message = BridgeUtils.Message({
            messageType: BridgeUtils.UPGRADE,
            version: 1,
            nonce: 0,
            chainID: _chainID,
            payload: payload
        });
        bytes memory encodedMessage = BridgeUtils.encodeMessage(message);
        bytes32 messageHash = keccak256(encodedMessage);
        bytes[] memory signatures = new bytes[](4);
        signatures[0] = getSignature(messageHash, committeeMemberPkA);
        signatures[1] = getSignature(messageHash, committeeMemberPkB);
        signatures[2] = getSignature(messageHash, committeeMemberPkC);
        signatures[3] = getSignature(messageHash, committeeMemberPkD);
        vm.expectRevert(bytes("CommitteeUpgradeable: Invalid proxy address"));
        bridge.upgradeWithSignatures(signatures, message);
    }

    function testInitializeImplementationDisabled() public {
        // Deploy the implementation contract without proxy
        address implementation = address(new BridgeCommittee());

        // Prepare initialization data
        address[] memory _committeeMembers = new address[](5);
        _committeeMembers[0] = committeeMemberA;
        _committeeMembers[1] = committeeMemberB;
        _committeeMembers[2] = committeeMemberC;
        _committeeMembers[3] = committeeMemberD;
        _committeeMembers[4] = committeeMemberE;
        uint16[] memory _stake = new uint16[](5);
        _stake[0] = 1000;
        _stake[1] = 1000;
        _stake[2] = 1000;
        _stake[3] = 2002;
        _stake[4] = 4998;

        // Try to call the initializer directly on the implementation contract
        // Expect it to revert because initializers should be disabled
        vm.expectRevert();
        BridgeCommittee(implementation).initialize(_committeeMembers, _stake, 100);
    }

    // An e2e upgrade regression test covering message ser/de and signature verification
    function testUpgradeRegressionTestWithV2Initializer() public {
        bytes memory messagePrefix = hex"5355495f4252494447455f4d455353414745050100000000000000000c";

        bytes memory initV2CallData =
            hex"000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000045cd8a76b00000000000000000000000000000000000000000000000000000000";

        bytes memory payload = abi.encodePacked(
            abi.encode(address(bridge)), abi.encode(address(bridgeV2)), initV2CallData
        );

        bytes memory encodedMessage = abi.encodePacked(messagePrefix, payload);

        bytes32 messageHash = keccak256(encodedMessage);

        // Create upgrade message
        BridgeUtils.Message memory message = BridgeUtils.Message({
            messageType: BridgeUtils.UPGRADE,
            version: 1,
            nonce: 0,
            chainID: _chainID,
            payload: payload
        });

        bytes[] memory signatures = new bytes[](4);
        signatures[0] = getSignature(messageHash, committeeMemberPkA);
        signatures[1] = getSignature(messageHash, committeeMemberPkB);
        signatures[2] = getSignature(messageHash, committeeMemberPkC);
        signatures[3] = getSignature(messageHash, committeeMemberPkD);

        bridge.upgradeWithSignatures(signatures, message);

        assertTrue(bridge.paused());
        assertEq(Upgrades.getImplementationAddress(address(bridge)), address(bridgeV2));
    }

    function testUpgradeRegressionTestWith1CalldataArg() public {
        bytes memory messagePrefix = hex"5355495f4252494447455f4d455353414745050100000000000000000c";

        bytes memory newMockFunc1CallData =
            hex"00000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000024417795ef000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000";

        bytes memory payload = abi.encodePacked(
            abi.encode(address(bridge)), abi.encode(address(bridgeV2)), newMockFunc1CallData
        );

        bytes memory encodedMessage = abi.encodePacked(messagePrefix, payload);

        bytes32 messageHash = keccak256(encodedMessage);

        // Create upgrade message
        BridgeUtils.Message memory message = BridgeUtils.Message({
            messageType: BridgeUtils.UPGRADE,
            version: 1,
            nonce: 0,
            chainID: _chainID,
            payload: payload
        });

        bytes[] memory signatures = new bytes[](4);
        signatures[0] = getSignature(messageHash, committeeMemberPkA);
        signatures[1] = getSignature(messageHash, committeeMemberPkB);
        signatures[2] = getSignature(messageHash, committeeMemberPkC);
        signatures[3] = getSignature(messageHash, committeeMemberPkD);

        bridge.upgradeWithSignatures(signatures, message);

        MockSuiBridgeV2 newBridgeV2 = MockSuiBridgeV2(address(bridge));
        assertTrue(newBridgeV2.isPausing());
        assertEq(Upgrades.getImplementationAddress(address(bridge)), address(bridgeV2));
    }

    function testUpgradeRegressionTestWith2CalldataArg() public {
        bytes memory messagePrefix = hex"5355495f4252494447455f4d455353414745050100000000000000000c";

        bytes memory newMockFunc2CallData =
            hex"00000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000044be8fc25d0000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000002a00000000000000000000000000000000000000000000000000000000";

        bytes memory payload = abi.encodePacked(
            abi.encode(address(bridge)), abi.encode(address(bridgeV2)), newMockFunc2CallData
        );

        bytes memory encodedMessage = abi.encodePacked(messagePrefix, payload);

        bytes32 messageHash = keccak256(encodedMessage);

        // Create upgrade message
        BridgeUtils.Message memory message = BridgeUtils.Message({
            messageType: BridgeUtils.UPGRADE,
            version: 1,
            nonce: 0,
            chainID: _chainID,
            payload: payload
        });

        bytes[] memory signatures = new bytes[](4);
        signatures[0] = getSignature(messageHash, committeeMemberPkA);
        signatures[1] = getSignature(messageHash, committeeMemberPkB);
        signatures[2] = getSignature(messageHash, committeeMemberPkC);
        signatures[3] = getSignature(messageHash, committeeMemberPkD);

        bridge.upgradeWithSignatures(signatures, message);

        MockSuiBridgeV2 newBridgeV2 = MockSuiBridgeV2(address(bridge));
        assertTrue(newBridgeV2.isPausing());
        assertEq(newBridgeV2.mock(), 42);
        assertEq(Upgrades.getImplementationAddress(address(bridge)), address(bridgeV2));
    }

    function testUpgradeRegressionTestWithNoCalldata() public {
        bytes memory messagePrefix = hex"5355495f4252494447455f4d455353414745050100000000000000000c";

        bytes memory emptyCallData =
            hex"00000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000000";

        bytes memory payload = abi.encodePacked(
            abi.encode(address(bridge)), abi.encode(address(bridgeV2)), emptyCallData
        );

        bytes memory encodedMessage = abi.encodePacked(messagePrefix, payload);

        bytes32 messageHash = keccak256(encodedMessage);

        // Create upgrade message
        BridgeUtils.Message memory message = BridgeUtils.Message({
            messageType: BridgeUtils.UPGRADE,
            version: 1,
            nonce: 0,
            chainID: _chainID,
            payload: payload
        });

        bytes[] memory signatures = new bytes[](4);
        signatures[0] = getSignature(messageHash, committeeMemberPkA);
        signatures[1] = getSignature(messageHash, committeeMemberPkB);
        signatures[2] = getSignature(messageHash, committeeMemberPkC);
        signatures[3] = getSignature(messageHash, committeeMemberPkD);

        bridge.upgradeWithSignatures(signatures, message);

        MockSuiBridgeV2(address(bridge));
        assertEq(Upgrades.getImplementationAddress(address(bridge)), address(bridgeV2));
    }
}
