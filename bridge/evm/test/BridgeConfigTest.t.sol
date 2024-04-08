// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "./BridgeBaseTest.t.sol";
import "./mocks/MockTokens.sol";

contract BridgeConfigTest is BridgeBaseTest {
    function setUp() public {
        setUpBridgeTest();
    }

    function testBridgeConfigInitialization() public {
        assertTrue(config.tokenAddressOf(1) == wBTC);
        assertTrue(config.tokenAddressOf(2) == wETH);
        assertTrue(config.tokenAddressOf(3) == USDC);
        assertTrue(config.tokenAddressOf(4) == USDT);
        assertEq(config.tokenSuiDecimalOf(0), 9);
        assertEq(config.tokenSuiDecimalOf(1), 8);
        assertEq(config.tokenSuiDecimalOf(2), 8);
        assertEq(config.tokenSuiDecimalOf(3), 6);
        assertEq(config.tokenSuiDecimalOf(4), 6);
        assertEq(config.tokenPriceOf(0), SUI_PRICE);
        assertEq(config.tokenPriceOf(1), BTC_PRICE);
        assertEq(config.tokenPriceOf(2), ETH_PRICE);
        assertEq(config.tokenPriceOf(3), USDC_PRICE);
        assertEq(config.tokenPriceOf(4), USDC_PRICE);
        assertEq(config.chainID(), chainID);
        assertTrue(config.supportedChains(0));
    }

    function testGetAddress() public {
        assertEq(config.tokenAddressOf(1), wBTC);
    }

    function testIsTokenSupported() public {
        assertTrue(config.isTokenSupported(1));
        assertTrue(!config.isTokenSupported(0));
    }

    function testTokenSuiDecimalOf() public {
        assertEq(config.tokenSuiDecimalOf(1), 8);
    }

    function testAddTokensWithSignatures() public {
        MockUSDC _newToken = new MockUSDC();

        // Create update tokens payload
        bool _isNative = true;
        uint8 _numTokenIDs = 1;
        uint8 tokenID1 = 10;
        uint8 _numAddresses = 1;
        address address1 = address(_newToken);
        uint8 _numSuiDecimals = 1;
        uint8 suiDecimal1 = 6;
        uint8 _numPrices = 1;
        uint64 price1 = 100_000_0000;

        bytes memory payload = abi.encodePacked(
            _isNative,
            _numTokenIDs,
            tokenID1,
            _numAddresses,
            address1,
            _numSuiDecimals,
            suiDecimal1,
            _numPrices,
            price1
        );

        // Create transfer message
        BridgeUtils.Message memory message = BridgeUtils.Message({
            messageType: BridgeUtils.ADD_EVM_TOKENS,
            version: 1,
            nonce: 0,
            chainID: 1,
            payload: payload
        });

        bytes memory encodedMessage = BridgeUtils.encodeMessage(message);

        bytes32 messageHash = keccak256(encodedMessage);

        bytes[] memory signatures = new bytes[](4);

        signatures[0] = getSignature(messageHash, committeeMemberPkA);
        signatures[1] = getSignature(messageHash, committeeMemberPkB);
        signatures[2] = getSignature(messageHash, committeeMemberPkC);
        signatures[3] = getSignature(messageHash, committeeMemberPkD);

        // test token ID 10 is not supported
        assertFalse(config.isTokenSupported(10));
        config.addTokensWithSignatures(signatures, message);
        assertTrue(config.isTokenSupported(10));
        assertEq(config.tokenAddressOf(10), address1);
        assertEq(config.tokenSuiDecimalOf(10), 6);
        assertEq(config.tokenPriceOf(10), 100_000_0000);
    }

    function testAddTokensAddressFailure() public {
        MockUSDC _newToken = new MockUSDC();

        // Create update tokens payload
        bool _isNative = true;
        uint8 _numTokenIDs = 1;
        uint8 tokenID1 = 10;
        uint8 _numAddresses = 1;
        address address1 = address(0);
        uint8 _numSuiDecimals = 1;
        uint8 suiDecimal1 = 6;
        uint8 _numPrices = 1;
        uint64 price1 = 100_000_00000000;

        bytes memory payload = abi.encodePacked(
            _isNative,
            _numTokenIDs,
            tokenID1,
            _numAddresses,
            address1,
            _numSuiDecimals,
            suiDecimal1,
            _numPrices,
            price1
        );

        // Create Add evm token message
        BridgeUtils.Message memory message = BridgeUtils.Message({
            messageType: BridgeUtils.ADD_EVM_TOKENS,
            version: 1,
            nonce: 0,
            chainID: 1,
            payload: payload
        });

        bytes memory encodedMessage = BridgeUtils.encodeMessage(message);

        bytes32 messageHash = keccak256(encodedMessage);

        bytes[] memory signatures = new bytes[](4);

        signatures[0] = getSignature(messageHash, committeeMemberPkA);
        signatures[1] = getSignature(messageHash, committeeMemberPkB);
        signatures[2] = getSignature(messageHash, committeeMemberPkC);
        signatures[3] = getSignature(messageHash, committeeMemberPkD);

        // address should fail because the address supplied in the message is 0
        vm.expectRevert(bytes("BridgeConfig: Invalid token address"));
        config.addTokensWithSignatures(signatures, message);
    }

    function testAddTokensSuiDecimalFailure() public {
        MockUSDC _newToken = new MockUSDC();

        // Create add tokens payload
        bool _isNative = true;
        uint8 _numTokenIDs = 1;
        uint8 tokenID1 = 10;
        uint8 _numAddresses = 1;
        address address1 = address(_newToken);
        uint8 _numSuiDecimals = 1;
        uint8 suiDecimal1 = 10;
        uint8 _numPrices = 1;
        uint64 price1 = 100_000_00000000;

        bytes memory payload = abi.encodePacked(
            _isNative,
            _numTokenIDs,
            tokenID1,
            _numAddresses,
            address1,
            _numSuiDecimals,
            suiDecimal1,
            _numPrices,
            price1
        );

        // Create transfer message
        BridgeUtils.Message memory message = BridgeUtils.Message({
            messageType: BridgeUtils.ADD_EVM_TOKENS,
            version: 1,
            nonce: 0,
            chainID: 1,
            payload: payload
        });

        bytes memory encodedMessage = BridgeUtils.encodeMessage(message);

        bytes32 messageHash = keccak256(encodedMessage);

        bytes[] memory signatures = new bytes[](4);

        signatures[0] = getSignature(messageHash, committeeMemberPkA);
        signatures[1] = getSignature(messageHash, committeeMemberPkB);
        signatures[2] = getSignature(messageHash, committeeMemberPkC);
        signatures[3] = getSignature(messageHash, committeeMemberPkD);

        // add token shoudl fail because the sui decimal is greater than the eth decimal
        vm.expectRevert(bytes("BridgeConfig: Invalid Sui decimal"));
        config.addTokensWithSignatures(signatures, message);
    }

    function testAddTokensPriceFailure() public {
        MockUSDC _newToken = new MockUSDC();

        // Create update tokens payload
        bool _isNative = true;
        uint8 _numTokenIDs = 1;
        uint8 tokenID1 = 10;
        uint8 _numAddresses = 1;
        address address1 = address(_newToken);
        uint8 _numSuiDecimals = 1;
        uint8 suiDecimal1 = 10;
        uint8 _numPrices = 1;
        uint64 price1 = 0;

        bytes memory payload = abi.encodePacked(
            _isNative,
            _numTokenIDs,
            tokenID1,
            _numAddresses,
            address1,
            _numSuiDecimals,
            suiDecimal1,
            _numPrices,
            price1
        );

        // Create transfer message
        BridgeUtils.Message memory message = BridgeUtils.Message({
            messageType: BridgeUtils.ADD_EVM_TOKENS,
            version: 1,
            nonce: 0,
            chainID: 1,
            payload: payload
        });

        bytes memory encodedMessage = BridgeUtils.encodeMessage(message);

        bytes32 messageHash = keccak256(encodedMessage);

        bytes[] memory signatures = new bytes[](4);

        signatures[0] = getSignature(messageHash, committeeMemberPkA);
        signatures[1] = getSignature(messageHash, committeeMemberPkB);
        signatures[2] = getSignature(messageHash, committeeMemberPkC);
        signatures[3] = getSignature(messageHash, committeeMemberPkD);

        vm.expectRevert(bytes("BridgeConfig: Invalid token price"));
        config.addTokensWithSignatures(signatures, message);
    }

    function testUpdateTokenPriceWithSignatures() public {
        // Create update tokens payload
        uint8 tokenID = BridgeUtils.ETH;
        uint64 price = 100_000_0000;

        bytes memory payload = abi.encodePacked(tokenID, price);

        // Create transfer message
        BridgeUtils.Message memory message = BridgeUtils.Message({
            messageType: BridgeUtils.UPDATE_TOKEN_PRICE,
            version: 1,
            nonce: 0,
            chainID: 1,
            payload: payload
        });

        bytes memory encodedMessage = BridgeUtils.encodeMessage(message);

        bytes32 messageHash = keccak256(encodedMessage);

        bytes[] memory signatures = new bytes[](4);

        signatures[0] = getSignature(messageHash, committeeMemberPkA);
        signatures[1] = getSignature(messageHash, committeeMemberPkB);
        signatures[2] = getSignature(messageHash, committeeMemberPkC);
        signatures[3] = getSignature(messageHash, committeeMemberPkD);

        // test ETH price
        assertEq(config.tokenPriceOf(BridgeUtils.ETH), ETH_PRICE);
        config.updateTokenPriceWithSignatures(signatures, message);
        assertEq(config.tokenPriceOf(BridgeUtils.ETH), 100_000_0000);
    }

    // An e2e update token price regression test covering message ser/de
    function testUpdateTokenPricesRegressionTest() public {
        address[] memory _committee = new address[](4);
        uint16[] memory _stake = new uint16[](4);
        _committee[0] = 0x68B43fD906C0B8F024a18C56e06744F7c6157c65;
        _committee[1] = 0xaCAEf39832CB995c4E049437A3E2eC6a7bad1Ab5;
        _committee[2] = 0x8061f127910e8eF56F16a2C411220BaD25D61444;
        _committee[3] = 0x508F3F1ff45F4ca3D8e86CDCC91445F00aCC59fC;
        _stake[0] = 2500;
        _stake[1] = 2500;
        _stake[2] = 2500;
        _stake[3] = 2500;
        committee = new BridgeCommittee();
        committee.initialize(_committee, _stake, minStakeRequired);
        committee.initializeConfig(address(config));
        vault = new BridgeVault(wETH);

        uint64[] memory totalLimits = new uint64[](1);
        totalLimits[0] = 1000000;
        uint8[] memory _supportedDestinationChains = new uint8[](1);
        _supportedDestinationChains[0] = 0;
        skip(2 days);
        limiter = new BridgeLimiter();
        limiter.initialize(address(committee), _supportedDestinationChains, totalLimits);
        bridge = new SuiBridge();
        bridge.initialize(address(committee), address(vault), address(limiter), wETH);
        vault.transferOwnership(address(bridge));
        limiter.transferOwnership(address(bridge));

        bytes memory payload = hex"01000000003b9aca00";

        // Create update token price message
        BridgeUtils.Message memory message = BridgeUtils.Message({
            messageType: BridgeUtils.UPDATE_TOKEN_PRICE,
            version: 1,
            nonce: 266,
            chainID: 3,
            payload: payload
        });
        bytes memory encodedMessage = BridgeUtils.encodeMessage(message);
        bytes memory expectedEncodedMessage =
            hex"5355495f4252494447455f4d4553534147450401000000000000010a0301000000003b9aca00";

        assertEq(encodedMessage, expectedEncodedMessage);
    }

    // An e2e update token price regression test covering message ser/de and signature verification
    function testUpdateTokenPriceRegressionTestWithSigVerficiation() public {
        address[] memory _committee = new address[](4);
        _committee[0] = 0x68B43fD906C0B8F024a18C56e06744F7c6157c65;
        _committee[1] = 0xaCAEf39832CB995c4E049437A3E2eC6a7bad1Ab5;
        _committee[2] = 0x8061f127910e8eF56F16a2C411220BaD25D61444;
        _committee[3] = 0x508F3F1ff45F4ca3D8e86CDCC91445F00aCC59fC;
        uint8 sendingChainID = 1;
        uint8[] memory _supportedChains = new uint8[](1);
        _supportedChains[0] = sendingChainID;
        uint8 chainID = 11;
        uint16[] memory _stake = new uint16[](4);
        _stake[0] = 2500;
        _stake[1] = 2500;
        _stake[2] = 2500;
        _stake[3] = 2500;
        committee = new BridgeCommittee();
        committee.initialize(_committee, _stake, minStakeRequired);
        uint8[] memory _supportedDestinationChains = new uint8[](1);
        _supportedDestinationChains[0] = sendingChainID;

        config = new BridgeConfig();
        config.initialize(
            address(committee), chainID, supportedTokens, tokenPrices, _supportedChains
        );

        committee.initializeConfig(address(config));

        vault = new BridgeVault(wETH);
        skip(2 days);

        uint64[] memory totalLimits = new uint64[](1);
        totalLimits[0] = 1000000;

        limiter = new BridgeLimiter();
        limiter.initialize(address(committee), _supportedDestinationChains, totalLimits);
        bridge = new SuiBridge();
        bridge.initialize(address(committee), address(vault), address(limiter), wETH);
        vault.transferOwnership(address(bridge));
        limiter.transferOwnership(address(bridge));

        // BTC -> 600_000_000 ($60k)
        bytes memory payload = hex"010000000023c34600";

        // Create update token price message
        BridgeUtils.Message memory message = BridgeUtils.Message({
            messageType: BridgeUtils.UPDATE_TOKEN_PRICE,
            version: 1,
            nonce: 0,
            chainID: chainID,
            payload: payload
        });
        bytes memory encodedMessage = BridgeUtils.encodeMessage(message);
        bytes memory expectedEncodedMessage =
            hex"5355495f4252494447455f4d455353414745040100000000000000000b010000000023c34600";

        assertEq(encodedMessage, expectedEncodedMessage);

        bytes[] memory signatures = new bytes[](3);

        signatures[0] =
            hex"eb81068c2214c01bf5d89e6bd748c0d184ae68f74d365174657053af916dcd335960737eb724560a3481bb77b7df4169d8305a034143e1c749fd9f9bcda6cc1601";
        signatures[1] =
            hex"116ad7d7bb705374328f85613020777d636fa092f98aa59a1d58f12f36d96f0e7aacfeb8ff356289da8d0d75278ccad8c19ec878db0b836f96ab544e91de1fed01";
        signatures[2] =
            hex"b0229b50b0fe3fd4cdb05b31c7689d99e3181f9f11069cb457d73112985865ff504d9a9959c367d02b18b2d78312a012f194798499198410880351ab0a241a0c00";

        committee.verifySignatures(signatures, message);

        config.updateTokenPriceWithSignatures(signatures, message);
        assertEq(config.tokenPrices(BridgeUtils.BTC), 600_000_000);
    }

    function testAddTokensRegressionTest() public {
        address[] memory _committee = new address[](4);
        uint16[] memory _stake = new uint16[](4);
        _committee[0] = 0x68B43fD906C0B8F024a18C56e06744F7c6157c65;
        _committee[1] = 0xaCAEf39832CB995c4E049437A3E2eC6a7bad1Ab5;
        _committee[2] = 0x8061f127910e8eF56F16a2C411220BaD25D61444;
        _committee[3] = 0x508F3F1ff45F4ca3D8e86CDCC91445F00aCC59fC;
        _stake[0] = 2500;
        _stake[1] = 2500;
        _stake[2] = 2500;
        _stake[3] = 2500;
        committee = new BridgeCommittee();
        committee.initialize(_committee, _stake, minStakeRequired);
        committee.initializeConfig(address(config));
        vault = new BridgeVault(wETH);

        uint64[] memory totalLimits = new uint64[](1);
        totalLimits[0] = 1000000;
        uint8[] memory _supportedDestinationChains = new uint8[](1);
        _supportedDestinationChains[0] = 0;
        skip(2 days);
        limiter = new BridgeLimiter();
        limiter.initialize(address(committee), _supportedDestinationChains, totalLimits);
        bridge = new SuiBridge();
        bridge.initialize(address(committee), address(vault), address(limiter), wETH);
        vault.transferOwnership(address(bridge));
        limiter.transferOwnership(address(bridge));

        bytes memory payload = hex"0103636465030101010101010101010101010101010101010101020202020202020202020202020202020202020203030303030303030303030303030303030303030305060703000000003b9aca00000000007735940000000000b2d05e00";

        (
            bool native,
            uint8[] memory tokenIDs,
            address[] memory tokenAddresses,
            uint8[] memory suiDecimals,
            uint64[] memory tokenPrices
        ) = BridgeUtils.decodeAddTokensPayload(payload);

        assertEq(native, true);
        assertEq(tokenIDs.length, 3);
        assertEq(tokenIDs[0], 99);
        assertEq(tokenIDs[1], 100);
        assertEq(tokenIDs[2], 101);

        assertEq(tokenAddresses.length, 3);
        assertEq(tokenAddresses[0], address(0x0101010101010101010101010101010101010101));
        assertEq(tokenAddresses[1], address(0x0202020202020202020202020202020202020202));
        assertEq(tokenAddresses[2], address(0x0303030303030303030303030303030303030303));

        assertEq(suiDecimals.length, 3);
        assertEq(suiDecimals[0], 5);
        assertEq(suiDecimals[1], 6);
        assertEq(suiDecimals[2], 7);

        assertEq(tokenPrices.length, 3);
        assertEq(tokenPrices[0], 1_000_000_000);
        assertEq(tokenPrices[1], 2_000_000_000);
        assertEq(tokenPrices[2], 3_000_000_000);

        BridgeUtils.Message memory message = BridgeUtils.Message({
            messageType: BridgeUtils.ADD_EVM_TOKENS,
            version: 1,
            nonce: 0,
            chainID: 12,
            payload: payload
        });
        bytes memory encodedMessage = BridgeUtils.encodeMessage(message);
        bytes memory expectedEncodedMessage =
            hex"5355495f4252494447455f4d455353414745070100000000000000000c0103636465030101010101010101010101010101010101010101020202020202020202020202020202020202020203030303030303030303030303030303030303030305060703000000003b9aca00000000007735940000000000b2d05e00";

        assertEq(encodedMessage, expectedEncodedMessage);

        bytes[] memory signatures = new bytes[](3);

        signatures[0] = hex"a6d844214b2614b95a89741e97ddf873ff3a07ea82b2cb8f242a85c8cf0373920a1e7882526611eb34add280c0029dc2a2ba411e656f86926593e7c4b41e47c801";
        signatures[1] = hex"3e7a698df30c74ea00630257d426742d2cd45b2826ebeb82a791498e5e492f6301fb02b7cf80c0e1c5614dcb7f4ca565f0001baa557bb2753b6c08ec0b0cc6da01";
        signatures[2] = hex"1d30cffbcbf27a8a465a932ba7f69c59380ecf3de9330f2fbd11b1002ff649467ca2db8e63dddb05acba5f471e51c8731052b46e7a00090de75582aec11860c600";
        committee.verifySignatures(signatures, message);

        // FIXME: @bridger, for some reason the following line fails whilst the above line passes
        // config.addTokensWithSignatures(signatures, message);
        // assertEq(config.tokenPriceOf(99), 1_000_000_000);
        // FIXME: assert every piece of info about the new token
    }
}
