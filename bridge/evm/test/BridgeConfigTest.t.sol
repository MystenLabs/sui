// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "./BridgeBaseTest.t.sol";

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

    function testUpdateTokenPriceWithSignatures() public {
        // Create update tokens payload
        uint8 tokenID = BridgeUtils.ETH;
        uint64 price = 100_000_0000;

        bytes memory payload = abi.encodePacked(tokenID, price);

        console.logBytes(payload);

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
}
