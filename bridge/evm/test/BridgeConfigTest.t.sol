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

    function testAddTokensWithSignatures() public {
        // Create update tokens payload
        bool _isNative = true;
        uint8 _numTokenIDs = 1;
        uint8 tokenID1 = 10;
        uint8 _numAddresses = 1;
        address address1 = address(10101);
        uint8 _numSuiDecimals = 1;
        uint8 suiDecimal1 = 8;
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

        console.logBytes(payload);

        // Create transfer message
        BridgeUtils.Message memory message = BridgeUtils.Message({
            messageType: BridgeUtils.ADD_TOKENS,
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
        assertEq(config.tokenAddressOf(10), address(10101));
        assertEq(config.tokenSuiDecimalOf(10), 8);
        assertEq(config.tokenPriceOf(10), 100_000_0000);
    }

    function testUpdateTokenPricesWithSignatures() public {
        // Create update tokens payload
        uint8 _numTokenIDs = 1;
        uint8 tokenID1 = BridgeUtils.ETH;
        uint8 _numPrices = 1;
        uint64 price1 = 100_000_0000;

        bytes memory payload = abi.encodePacked(_numTokenIDs, tokenID1, _numPrices, price1);

        console.logBytes(payload);

        // Create transfer message
        BridgeUtils.Message memory message = BridgeUtils.Message({
            messageType: BridgeUtils.UPDATE_TOKEN_PRICES,
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
        config.updateTokenPricesWithSignatures(signatures, message);
        assertEq(config.tokenPriceOf(BridgeUtils.ETH), 100_000_0000);
    }

    // An e2e update token price regression test covering message ser/de and signature verification
    function testUpdateTokenPriceRegressionTest() public {
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

        // Fill vault with WETH
        changePrank(deployer);
        IWETH9(wETH).deposit{value: 10 ether}();
        IERC20(wETH).transfer(address(vault), 10 ether);

        bytes memory payload = hex"01000000003b9aca00";

        // Create transfer message
        BridgeUtils.Message memory message = BridgeUtils.Message({
            messageType: BridgeUtils.UPDATE_TOKEN_PRICES,
            version: 1,
            nonce: 266,
            chainID: 3,
            payload: payload
        });
        bytes memory encodedMessage = BridgeUtils.encodeMessage(message);
        bytes memory expectedEncodedMessage =
            hex"5355495f4252494447455f4d4553534147450401000000000000010a0301000000003b9aca00";

        assertEq(encodedMessage, expectedEncodedMessage);

        bytes[] memory signatures = new bytes[](2);

        // TODO: generate signatures
        // signatures[0] =
        //     hex"e1cf11b380855ff1d4a451ebc2fd68477cf701b7d4ec88da3082709fe95201a5061b4b60cf13815a80ba9dfead23e220506aa74c4a863ba045d95715b4cc6b6e00";
        // signatures[1] =
        //     hex"8ba9ec92c2d5a44ecc123182f689b901a93921fd35f581354fea20b25a0ded6d055b96a64bdda77dd5a62b93d29abe93640aa3c1a136348093cd7a2418c6bfa301";

        // committee.verifySignatures(signatures, message);

        // limiter.updateTokenPriceWithSignatures(signatures, message);
        // assertEq(limiter.tokenPrices(BridgeUtils.BTC), 100_000_0000);
    }

    function testAddTokensRegressionTest() public {}
}
