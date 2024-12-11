// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "./BridgeBaseTest.t.sol";
import "../contracts/utils/BridgeUtils.sol";

contract BridgeCommitteeTest is BridgeBaseTest {
    // This function is called before each unit test
    function setUp() public {
        setUpBridgeTest();
    }

    function testBridgeCommitteeInitialization() public {
        assertEq(committee.committeeStake(committeeMemberA), 1000);
        assertEq(committee.committeeStake(committeeMemberB), 1000);
        assertEq(committee.committeeStake(committeeMemberC), 1000);
        assertEq(committee.committeeStake(committeeMemberD), 2002);
        assertEq(committee.committeeStake(committeeMemberE), 4998);
        // Assert that the total stake is 10,000
        assertEq(
            committee.committeeStake(committeeMemberA) + committee.committeeStake(committeeMemberB)
                + committee.committeeStake(committeeMemberC)
                + committee.committeeStake(committeeMemberD)
                + committee.committeeStake(committeeMemberE),
            10000
        );
        // Check that the blocklist and nonces are initialized to zero
        assertEq(committee.blocklist(address(committeeMemberA)), false);
        assertEq(committee.blocklist(address(committeeMemberB)), false);
        assertEq(committee.blocklist(address(committeeMemberC)), false);
        assertEq(committee.blocklist(address(committeeMemberD)), false);
        assertEq(committee.blocklist(address(committeeMemberE)), false);
        assertEq(committee.nonces(0), 0);
        assertEq(committee.nonces(1), 0);
        assertEq(committee.nonces(2), 0);
        assertEq(committee.nonces(3), 0);
        assertEq(committee.nonces(4), 0);
    }

    function testBridgeCommitteeInitializationLength() public {
        address[] memory _committeeMembers = new address[](256);

        for (uint160 i = 0; i < 256; i++) {
            _committeeMembers[i] = address(i);
        }

        address _committee = Upgrades.deployUUPSProxy("BridgeCommittee.sol", "", opts);

        vm.expectRevert(bytes("BridgeCommittee: Committee length must be less than 256"));
        BridgeCommittee(_committee).initialize(
            _committeeMembers, new uint16[](256), minStakeRequired
        );
    }

    function testBridgeCommitteeInitializeConfig() public {
        vm.expectRevert(bytes("BridgeCommittee: Config already initialized"));
        // Initialize the committee with the config contract
        committee.initializeConfig(address(101));
    }

    function testBridgeFailInitialization() public {
        // Test fail initialize: Committee Duplicate Committee Member
        address[] memory _committeeDuplicateCommitteeMember = new address[](5);
        _committeeDuplicateCommitteeMember[0] = committeeMemberA;
        _committeeDuplicateCommitteeMember[1] = committeeMemberB;
        _committeeDuplicateCommitteeMember[2] = committeeMemberC;
        _committeeDuplicateCommitteeMember[3] = committeeMemberD;
        _committeeDuplicateCommitteeMember[4] = committeeMemberA;

        uint16[] memory _stakeDuplicateCommitteeMember = new uint16[](5);
        _stakeDuplicateCommitteeMember[0] = 1000;
        _stakeDuplicateCommitteeMember[1] = 1000;
        _stakeDuplicateCommitteeMember[2] = 1000;
        _stakeDuplicateCommitteeMember[3] = 2002;
        _stakeDuplicateCommitteeMember[4] = 1000;

        address _committee = Upgrades.deployUUPSProxy("BridgeCommittee.sol", "", opts);

        committee = BridgeCommittee(_committee);

        vm.expectRevert(bytes("BridgeCommittee: Duplicate committee member"));

        committee.initialize(
            _committeeDuplicateCommitteeMember, _stakeDuplicateCommitteeMember, minStakeRequired
        );

        address[] memory _committeeNotSameLength = new address[](5);
        _committeeNotSameLength[0] = committeeMemberA;
        _committeeNotSameLength[1] = committeeMemberB;
        _committeeNotSameLength[2] = committeeMemberC;
        _committeeNotSameLength[3] = committeeMemberD;
        _committeeNotSameLength[4] = committeeMemberE;

        uint16[] memory _stakeNotSameLength = new uint16[](4);
        _stakeNotSameLength[0] = 1000;
        _stakeNotSameLength[1] = 1000;
        _stakeNotSameLength[2] = 1000;
        _stakeNotSameLength[3] = 2002;

        vm.expectRevert(
            bytes("BridgeCommittee: Committee and stake arrays must be of the same length")
        );

        committee.initialize(_committeeNotSameLength, _stakeNotSameLength, minStakeRequired);
    }

    function testVerifySignaturesWithValidSignatures() public {
        // Create a message
        BridgeUtils.Message memory message = BridgeUtils.Message({
            messageType: BridgeUtils.TOKEN_TRANSFER,
            version: 1,
            nonce: 1,
            chainID: chainID,
            payload: "0x0"
        });

        bytes memory messageBytes = BridgeUtils.encodeMessage(message);

        bytes32 messageHash = keccak256(messageBytes);

        bytes[] memory signatures = new bytes[](4);

        // Create signatures from A - D
        signatures[0] = getSignature(messageHash, committeeMemberPkA);
        signatures[1] = getSignature(messageHash, committeeMemberPkB);
        signatures[2] = getSignature(messageHash, committeeMemberPkC);
        signatures[3] = getSignature(messageHash, committeeMemberPkD);

        // Call the verifySignatures function and it would not revert
        committee.verifySignatures(signatures, message);
    }

    function testVerifySignaturesWithInvalidSignatures() public {
        // Create a message
        BridgeUtils.Message memory message = BridgeUtils.Message({
            messageType: BridgeUtils.TOKEN_TRANSFER,
            version: 1,
            nonce: 1,
            chainID: chainID,
            payload: "0x0"
        });

        bytes memory messageBytes = BridgeUtils.encodeMessage(message);

        bytes32 messageHash = keccak256(messageBytes);

        bytes[] memory signatures = new bytes[](3);

        // Create signatures from A - D
        signatures[0] = getSignature(messageHash, committeeMemberPkA);
        signatures[1] = getSignature(messageHash, committeeMemberPkB);
        signatures[2] = getSignature(messageHash, committeeMemberPkC);

        // Call the verifySignatures function and expect it to revert
        vm.expectRevert(bytes("BridgeCommittee: Insufficient stake amount"));
        committee.verifySignatures(signatures, message);
    }

    function testVerifySignaturesDuplicateSignature() public {
        // Create a message
        BridgeUtils.Message memory message = BridgeUtils.Message({
            messageType: BridgeUtils.TOKEN_TRANSFER,
            version: 1,
            nonce: 1,
            chainID: chainID,
            payload: "0x0"
        });

        bytes memory messageBytes = BridgeUtils.encodeMessage(message);
        bytes32 messageHash = keccak256(messageBytes);

        bytes[] memory signatures = new bytes[](4);

        // Create signatures from A - C
        signatures[0] = getSignature(messageHash, committeeMemberPkA);
        signatures[1] = getSignature(messageHash, committeeMemberPkA);
        signatures[2] = getSignature(messageHash, committeeMemberPkB);
        signatures[3] = getSignature(messageHash, committeeMemberPkC);

        // Call the verifySignatures function and expect it to revert
        vm.expectRevert(bytes("BridgeCommittee: Duplicate signature provided"));
        committee.verifySignatures(signatures, message);
    }

    function testFailUpdateBlocklistWithSignaturesInvalidNonce() public {
        // create payload
        address[] memory _blocklist = new address[](1);
        _blocklist[0] = committeeMemberA;
        bytes memory payload = abi.encode(uint8(0), _blocklist);

        // Create a message with wrong nonce
        BridgeUtils.Message memory messageWrongNonce = BridgeUtils.Message({
            messageType: BridgeUtils.BLOCKLIST,
            version: 1,
            nonce: 0,
            chainID: chainID,
            payload: payload
        });
        bytes memory messageBytes = BridgeUtils.encodeMessage(messageWrongNonce);
        bytes32 messageHash = keccak256(messageBytes);
        bytes[] memory signatures = new bytes[](4);

        // Create signatures from A - D
        signatures[0] = getSignature(messageHash, committeeMemberPkA);
        signatures[1] = getSignature(messageHash, committeeMemberPkB);
        signatures[2] = getSignature(messageHash, committeeMemberPkC);
        signatures[3] = getSignature(messageHash, committeeMemberPkD);
        vm.expectRevert(bytes("BridgeCommittee: Invalid nonce"));
        committee.updateBlocklistWithSignatures(signatures, messageWrongNonce);
    }

    function testUpdateBlocklistWithSignaturesMessageDoesNotMatchType() public {
        // create payload
        address[] memory _blocklist = new address[](1);
        _blocklist[0] = committeeMemberA;
        bytes memory payload = abi.encode(uint8(0), _blocklist);

        // Create a message with wrong messageType
        BridgeUtils.Message memory messageWrongMessageType = BridgeUtils.Message({
            messageType: BridgeUtils.TOKEN_TRANSFER,
            version: 1,
            nonce: 0,
            chainID: chainID,
            payload: payload
        });
        bytes memory messageBytes = BridgeUtils.encodeMessage(messageWrongMessageType);
        bytes32 messageHash = keccak256(messageBytes);
        bytes[] memory signatures = new bytes[](4);

        // Create signatures from A - D
        signatures[0] = getSignature(messageHash, committeeMemberPkA);
        signatures[1] = getSignature(messageHash, committeeMemberPkB);
        signatures[2] = getSignature(messageHash, committeeMemberPkC);
        signatures[3] = getSignature(messageHash, committeeMemberPkD);
        vm.expectRevert(bytes("MessageVerifier: message does not match type"));
        committee.updateBlocklistWithSignatures(signatures, messageWrongMessageType);
    }

    function testFailUpdateBlocklistWithSignaturesInvalidSignatures() public {
        // create payload
        address[] memory _blocklist = new address[](1);
        _blocklist[0] = committeeMemberA;
        bytes memory payload = abi.encode(uint8(0), _blocklist);

        // Create a message
        BridgeUtils.Message memory message = BridgeUtils.Message({
            messageType: BridgeUtils.BLOCKLIST,
            version: 1,
            nonce: 0,
            chainID: chainID,
            payload: payload
        });
        bytes memory messageBytes = BridgeUtils.encodeMessage(message);
        bytes32 messageHash = keccak256(messageBytes);
        bytes[] memory signatures = new bytes[](4);

        // Create signatures from A
        signatures[0] = getSignature(messageHash, committeeMemberPkA);
        vm.expectRevert(bytes("BridgeCommittee: Invalid signatures"));
        committee.updateBlocklistWithSignatures(signatures, message);
    }

    function testAddToBlocklist() public {
        // create payload
        address[] memory _blocklist = new address[](1);
        _blocklist[0] = committeeMemberA;
        bytes memory payload = hex"0001";
        payload = abi.encodePacked(payload, committeeMemberA);

        // Create a message
        BridgeUtils.Message memory message = BridgeUtils.Message({
            messageType: BridgeUtils.BLOCKLIST,
            version: 1,
            nonce: 0,
            chainID: chainID,
            payload: payload
        });

        bytes memory messageBytes = BridgeUtils.encodeMessage(message);
        bytes32 messageHash = keccak256(messageBytes);
        bytes[] memory signatures = new bytes[](4);

        // Create signatures from A - D
        signatures[0] = getSignature(messageHash, committeeMemberPkA);
        signatures[1] = getSignature(messageHash, committeeMemberPkB);
        signatures[2] = getSignature(messageHash, committeeMemberPkC);
        signatures[3] = getSignature(messageHash, committeeMemberPkD);

        committee.updateBlocklistWithSignatures(signatures, message);

        assertTrue(committee.blocklist(committeeMemberA));

        // update message
        message.nonce = 1;
        // reconstruct signatures
        messageBytes = BridgeUtils.encodeMessage(message);
        messageHash = keccak256(messageBytes);
        signatures[0] = getSignature(messageHash, committeeMemberPkA);
        signatures[1] = getSignature(messageHash, committeeMemberPkB);
        signatures[2] = getSignature(messageHash, committeeMemberPkC);
        signatures[3] = getSignature(messageHash, committeeMemberPkD);
        // verify CommitteeMemberA's signature is no longer valid
        vm.expectRevert(bytes("BridgeCommittee: Signer is blocklisted"));
        // re-verify signatures
        committee.verifySignatures(signatures, message);
    }

    function testSignerNotCommitteeMember() public {
        // create payload
        bytes memory payload = abi.encode(committeeMemberA);

        // Create a message
        BridgeUtils.Message memory message = BridgeUtils.Message({
            messageType: BridgeUtils.UPGRADE,
            version: 1,
            nonce: 0,
            chainID: chainID,
            payload: payload
        });

        bytes memory messageBytes = BridgeUtils.encodeMessage(message);
        bytes32 messageHash = keccak256(messageBytes);
        bytes[] memory signatures = new bytes[](4);

        (, uint256 committeeMemberPkF) = makeAddrAndKey("f");

        // Create signatures from A - D, and F
        signatures[0] = getSignature(messageHash, committeeMemberPkA);
        signatures[1] = getSignature(messageHash, committeeMemberPkB);
        signatures[2] = getSignature(messageHash, committeeMemberPkC);
        signatures[3] = getSignature(messageHash, committeeMemberPkF);

        vm.expectRevert(bytes("BridgeCommittee: Signer has no stake"));
        committee.verifySignatures(signatures, message);
    }

    function testRemoveFromBlocklist() public {
        testAddToBlocklist();

        // create payload
        address[] memory _blocklist = new address[](1);
        _blocklist[0] = committeeMemberA;
        bytes memory payload = hex"0101";
        payload = abi.encodePacked(payload, committeeMemberA);

        // Create a message
        BridgeUtils.Message memory message = BridgeUtils.Message({
            messageType: BridgeUtils.BLOCKLIST,
            version: 1,
            nonce: 1,
            chainID: chainID,
            payload: payload
        });

        bytes memory messageBytes = BridgeUtils.encodeMessage(message);
        bytes32 messageHash = keccak256(messageBytes);
        bytes[] memory signatures = new bytes[](4);

        // Create signatures from B - E
        signatures[0] = getSignature(messageHash, committeeMemberPkB);
        signatures[1] = getSignature(messageHash, committeeMemberPkC);
        signatures[2] = getSignature(messageHash, committeeMemberPkD);
        signatures[3] = getSignature(messageHash, committeeMemberPkE);

        committee.updateBlocklistWithSignatures(signatures, message);

        // verify CommitteeMemberA is no longer blocklisted
        assertFalse(committee.blocklist(committeeMemberA));
    }

    // An e2e update committee blocklist regression test covering message ser/de
    function testUpdateCommitteeBlocklistRegressionTest() public {
        bytes memory payload =
            hex"010268b43fd906c0b8f024a18c56e06744f7c6157c65acaef39832cb995c4e049437a3e2ec6a7bad1ab5";
        // Create blocklist message
        BridgeUtils.Message memory message = BridgeUtils.Message({
            messageType: BridgeUtils.BLOCKLIST,
            version: 1,
            nonce: 68,
            chainID: 2,
            payload: payload
        });
        bytes memory encodedMessage = BridgeUtils.encodeMessage(message);
        bytes memory expectedEncodedMessage =
            hex"5355495f4252494447455f4d4553534147450101000000000000004402010268b43fd906c0b8f024a18c56e06744f7c6157c65acaef39832cb995c4e049437a3e2ec6a7bad1ab5";

        assertEq(encodedMessage, expectedEncodedMessage);
    }

    // An e2e update committee blocklist regression test covering message ser/de and signature verification
    function testUpdateCommitteeBlocklistRegressionTestWithSignatures() public {
        address[] memory _committeeList = new address[](4);
        uint16[] memory _stake = new uint16[](4);
        uint8 chainID = 11;
        _committeeList[0] = 0x68B43fD906C0B8F024a18C56e06744F7c6157c65;
        _committeeList[1] = 0xaCAEf39832CB995c4E049437A3E2eC6a7bad1Ab5;
        _committeeList[2] = 0x8061f127910e8eF56F16a2C411220BaD25D61444;
        _committeeList[3] = 0x508F3F1ff45F4ca3D8e86CDCC91445F00aCC59fC;
        _stake[0] = 2500;
        _stake[1] = 2500;
        _stake[2] = 2500;
        _stake[3] = 2500;
        address _committee = Upgrades.deployUUPSProxy(
            "BridgeCommittee.sol",
            abi.encodeCall(BridgeCommittee.initialize, (_committeeList, _stake, minStakeRequired)),
            opts
        );
        committee = BridgeCommittee(_committee);

        address _config = Upgrades.deployUUPSProxy(
            "BridgeConfig.sol",
            abi.encodeCall(
                BridgeConfig.initialize,
                (address(committee), chainID, supportedTokens, tokenPrices, tokenIds, suiDecimals, supportedChains)
            ),
            opts
        );

        committee.initializeConfig(_config);

        assertEq(committee.blocklist(0x68B43fD906C0B8F024a18C56e06744F7c6157c65), false);

        // blocklist 1 member 02321ede33d2c2d7a8a152f275a1484edef2098f034121a602cb7d767d38680aa4 ("0x68B43fD906C0B8F024a18C56e06744F7c6157c65")
        bytes memory payload = hex"000168b43fd906c0b8f024a18c56e06744f7c6157c65";
        BridgeUtils.Message memory message = BridgeUtils.Message({
            messageType: BridgeUtils.BLOCKLIST,
            version: 1,
            nonce: 0,
            chainID: chainID,
            payload: payload
        });
        bytes memory encodedMessage = BridgeUtils.encodeMessage(message);
        bytes memory expectedEncodedMessage =
            hex"5355495f4252494447455f4d455353414745010100000000000000000b000168b43fd906c0b8f024a18c56e06744f7c6157c65";

        assertEq(encodedMessage, expectedEncodedMessage);

        bytes[] memory signatures = new bytes[](3);

        signatures[0] =
            hex"add4b78733cc1cbf4e50b7f6b60c60370ed43fd57e016f478d49ed5050960c6b0099fc474e4ac92a5f260bd35e2a5870a2ec515897f2b0222ef601658210823400";
        signatures[1] =
            hex"7d16301c6ed6de65d9276f6135511102ff2917a97e5ca9fd2bf5a04fa80b0b4530818c3aec19d8da4331b2d5bac08e502507c0ce4e3e60cf9f993196f2123b7e01";
        signatures[2] =
            hex"753ae3fc2c22c7254cc9418461345a0bd9c83528d7b2988f03976b839a01e2532b91eefa5cfeeb209cf520329f89ad490cd752cfc9faad1d15df408093b23cd001";

        committee.verifySignatures(signatures, message);

        committee.updateBlocklistWithSignatures(signatures, message);

        assertEq(committee.blocklist(0x68B43fD906C0B8F024a18C56e06744F7c6157c65), true);

        // unblocklist 1 member 02321ede33d2c2d7a8a152f275a1484edef2098f034121a602cb7d767d38680aa4 ("0x68B43fD906C0B8F024a18C56e06744F7c6157c65")
        payload = hex"010168b43fd906c0b8f024a18c56e06744f7c6157c65";
        message = BridgeUtils.Message({
            messageType: BridgeUtils.BLOCKLIST,
            version: 1,
            nonce: 1,
            chainID: chainID,
            payload: payload
        });
        encodedMessage = BridgeUtils.encodeMessage(message);
        expectedEncodedMessage =
            hex"5355495f4252494447455f4d455353414745010100000000000000010b010168b43fd906c0b8f024a18c56e06744f7c6157c65";

        assertEq(encodedMessage, expectedEncodedMessage);

        signatures = new bytes[](3);

        // Note sig[0] is from blocklisetd validator, and it does not count.
        signatures[0] =
            hex"2b62c1b5e17de7f4baeec0f1e9c01107b799edb3070c2c1f00e41c9c1eb550c82ce168d2d635fd8b6999b6bd8f8ec31bcc86d4b13dc094c713c7f0f111d21dad00";
        signatures[1] =
            hex"0fc3cc67cb21dac7b7a5ef93a54b9e7b1057cab45cf62b8bd0f6dd217cf99f001d1ebdcf2751ec95d24829403b87ba6f0e603ebf6d98595048474837f9c40a8c00";
        signatures[2] =
            hex"62b36dab0d2c10f74d84b5f9838435c396cca1f3c4939eb4df82d1c72430e7ec2a030a980a9514beaeda6dffdc5e177b7edbd18543979f488d8fd09dba753a5500";

        vm.expectRevert(bytes("BridgeCommittee: Signer is blocklisted"));
        committee.verifySignatures(signatures, message);

        // use sig from a unblocklisted validator
        signatures[0] =
            hex"5f2bef9593c37589c18519e2b97c735e60e3ef26471d07e804fb259ed75beb7e0ad180d932ef8af6885a544ded4e372d75451667c238d8e7215454f8bdbebd3401";
        committee.verifySignatures(signatures, message);
        committee.updateBlocklistWithSignatures(signatures, message);
        assertEq(committee.blocklist(0x68B43fD906C0B8F024a18C56e06744F7c6157c65), false);
    }
}
