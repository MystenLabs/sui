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
        uint64 price1 = 100_000 * USD_VALUE_MULTIPLIER;

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
        assertEq(config.tokenPriceOf(10), 100_000 * USD_VALUE_MULTIPLIER);
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
        uint64 price1 = 100_000 * USD_VALUE_MULTIPLIER;

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
        uint64 price1 = 100_000 * USD_VALUE_MULTIPLIER;

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
        uint64 price = 100_000 * USD_VALUE_MULTIPLIER;

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
        assertEq(config.tokenPriceOf(BridgeUtils.ETH), 100_000 * USD_VALUE_MULTIPLIER);
    }

    // An e2e update token price regression test covering message ser/de
    function testUpdateTokenPricesRegressionTest() public {
        address[] memory _committeeList = new address[](4);
        uint16[] memory _stake = new uint16[](4);
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
        committee.initializeConfig(address(config));
        vault = new BridgeVault(wETH);

        uint64[] memory totalLimits = new uint64[](1);
        totalLimits[0] = 1000000;
        uint8[] memory _supportedDestinationChains = new uint8[](1);
        _supportedDestinationChains[0] = 0;
        skip(2 days);
        address _limiter = Upgrades.deployUUPSProxy(
            "BridgeLimiter.sol",
            abi.encodeCall(
                BridgeLimiter.initialize,
                (address(committee), _supportedDestinationChains, totalLimits)
            ),
            opts
        );
        limiter = BridgeLimiter(_limiter);
        address _suiBridge = Upgrades.deployUUPSProxy(
            "SuiBridge.sol",
            abi.encodeCall(
                SuiBridge.initialize, (address(committee), address(vault), address(limiter))
            ),
            opts
        );
        bridge = SuiBridge(_suiBridge);
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
        address[] memory _committeeList = new address[](4);
        _committeeList[0] = 0x68B43fD906C0B8F024a18C56e06744F7c6157c65;
        _committeeList[1] = 0xaCAEf39832CB995c4E049437A3E2eC6a7bad1Ab5;
        _committeeList[2] = 0x8061f127910e8eF56F16a2C411220BaD25D61444;
        _committeeList[3] = 0x508F3F1ff45F4ca3D8e86CDCC91445F00aCC59fC;
        uint8 sendingChainID = 1;
        uint8[] memory _supportedChains = new uint8[](1);
        _supportedChains[0] = sendingChainID;
        uint8 chainID = 11;
        uint16[] memory _stake = new uint16[](4);
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
        uint8[] memory _supportedDestinationChains = new uint8[](1);
        _supportedDestinationChains[0] = sendingChainID;

        address _config = Upgrades.deployUUPSProxy(
            "BridgeConfig.sol",
            abi.encodeCall(
                BridgeConfig.initialize,
                (address(committee), chainID, supportedTokens, tokenPrices, tokenIds, suiDecimals, _supportedDestinationChains)
            ),
            opts
        );
        config = BridgeConfig(_config);

        committee.initializeConfig(_config);

        vault = new BridgeVault(wETH);
        skip(2 days);

        uint64[] memory totalLimits = new uint64[](1);
        totalLimits[0] = 1000000;

        address _limiter = Upgrades.deployUUPSProxy(
            "BridgeLimiter.sol",
            abi.encodeCall(
                BridgeLimiter.initialize,
                (address(committee), _supportedDestinationChains, totalLimits)
            ),
            opts
        );
        limiter = BridgeLimiter(_limiter);
        address _suiBridge = Upgrades.deployUUPSProxy(
            "SuiBridge.sol",
            abi.encodeCall(
                SuiBridge.initialize, (address(committee), address(vault), address(limiter))
            ),
            opts
        );
        bridge = SuiBridge(_suiBridge);
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
        address[] memory _committeeList = new address[](4);
        uint16[] memory _stake = new uint16[](4);
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
        uint8[] memory _supportedDestinationChains = new uint8[](1);
        _supportedDestinationChains[0] = 0;
        address _config = Upgrades.deployUUPSProxy(
            "BridgeConfig.sol",
            abi.encodeCall(
                BridgeConfig.initialize,
                (address(committee), 12, supportedTokens, tokenPrices, tokenIds, suiDecimals, _supportedDestinationChains)
            ),
            opts
        );
        config = BridgeConfig(_config);

        committee.initializeConfig(address(config));
        vault = new BridgeVault(wETH);

        uint64[] memory totalLimits = new uint64[](1);
        totalLimits[0] = 1000000;

        skip(2 days);
        address _limiter = Upgrades.deployUUPSProxy(
            "BridgeLimiter.sol",
            abi.encodeCall(
                BridgeLimiter.initialize,
                (address(committee), _supportedDestinationChains, totalLimits)
            ),
            opts
        );
        limiter = BridgeLimiter(_limiter);
        address _suiBridge = Upgrades.deployUUPSProxy(
            "SuiBridge.sol",
            abi.encodeCall(
                SuiBridge.initialize, (address(committee), address(vault), address(limiter))
            ),
            opts
        );
        bridge = SuiBridge(_suiBridge);
        vault.transferOwnership(address(bridge));
        limiter.transferOwnership(address(bridge));

        bytes memory payload =
            hex"0103636465036b175474e89094c44da98b954eedeac495271d0fae7ab96520de3a18e5e111b5eaab095312d7fe84c18360217d8f7ab5e7c516566761ea12ce7f9d720305060703000000003b9aca00000000007735940000000000b2d05e00";

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
        assertEq(tokenAddresses[0], 0x6B175474E89094C44Da98b954EedeAC495271d0F); // dai
        assertEq(tokenAddresses[1], 0xae7ab96520DE3A18E5e111B5EaAb095312D7fE84); // lido
        assertEq(tokenAddresses[2], 0xC18360217D8F7Ab5e7c516566761Ea12Ce7F9D72); // ENS

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
            hex"5355495f4252494447455f4d455353414745070100000000000000000c0103636465036b175474e89094c44da98b954eedeac495271d0fae7ab96520de3a18e5e111b5eaab095312d7fe84c18360217d8f7ab5e7c516566761ea12ce7f9d720305060703000000003b9aca00000000007735940000000000b2d05e00";

        assertEq(encodedMessage, expectedEncodedMessage);

        bytes[] memory signatures = new bytes[](3);

        signatures[0] =
            hex"98b064aa172d0a66142f2fc45d9cd3255fb096cb92e0fcc9be4688b425aad6b53251c9044de4475e64e85b38b32cd3c813a8010281b00811d40fce9b3b372f2200";
        signatures[1] =
            hex"275037d70185c835b0d1ee70a118d1cc5da90db2468fab1fa24517eeec3055d814f0ca65db7e6274dbda92d33c9df914db7ada4901a283ec1d3e8c126827923600";
        signatures[2] =
            hex"ebb6669c8fb4b000fd41dde6e464c44c009ddcb47c05e7e5ea3deba71b21bd28156b23b6e7813a0603c57553ce484771c142ba6c981c4753035655e89006c0ee01";

        config.addTokensWithSignatures(signatures, message);

        assertEq(config.tokenPriceOf(99), 1_000_000_000);
        assertEq(config.tokenPriceOf(100), 2_000_000_000);
        assertEq(config.tokenPriceOf(101), 3_000_000_000);
        assertEq(config.tokenSuiDecimalOf(99), 5);
        assertEq(config.tokenSuiDecimalOf(100), 6);
        assertEq(config.tokenSuiDecimalOf(101), 7);
        assertEq(config.tokenAddressOf(99), 0x6B175474E89094C44Da98b954EedeAC495271d0F);
        assertEq(config.tokenAddressOf(100), 0xae7ab96520DE3A18E5e111B5EaAb095312D7fE84);
        assertEq(config.tokenAddressOf(101), 0xC18360217D8F7Ab5e7c516566761Ea12Ce7F9D72);
    }
}
