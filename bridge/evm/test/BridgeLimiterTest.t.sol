// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import "./BridgeBaseTest.t.sol";

contract BridgeLimiterTest is BridgeBaseTest {
    uint8 public supportedChainID;

    function setUp() public {
        setUpBridgeTest();
        // warp to next nearest hour start
        vm.warp(block.timestamp - (block.timestamp % 1 hours));
        supportedChainID = 0;
    }

    function testBridgeLimiterInitialization() public {
        assertEq(limiter.oldestChainTimestamp(supportedChainID), uint32(block.timestamp / 1 hours));
        assertEq(limiter.chainLimits(supportedChainID), totalLimit);
    }

    function testCalculateAmountInUSD() public {
        uint8 tokenID = 1; // wBTC
        uint256 wBTCAmount = 1_00000000; // wBTC has 8 decimals
        uint256 actual = limiter.calculateAmountInUSD(tokenID, wBTCAmount);
        assertEq(actual, BTC_PRICE);
        tokenID = 2;
        uint256 ethAmount = 1 ether;
        actual = limiter.calculateAmountInUSD(tokenID, ethAmount);
        assertEq(actual, ETH_PRICE);
        tokenID = 3;
        uint256 usdcAmount = 1_000000; // USDC has 6 decimals
        actual = limiter.calculateAmountInUSD(tokenID, usdcAmount);
        assertEq(actual, USDC_PRICE);
    }

    function testCalculateWindowLimit() public {
        changePrank(address(bridge));
        uint8 tokenID = 3;
        uint256 amount = 1_000000; // USDC has 6 decimals
        limiter.recordBridgeTransfers(supportedChainID, tokenID, amount);
        skip(1 hours);
        limiter.recordBridgeTransfers(supportedChainID, tokenID, 2 * amount);
        skip(1 hours);
        uint256 actual = limiter.calculateWindowAmount(supportedChainID);
        assertEq(actual, 3 * USD_VALUE_MULTIPLIER);
        skip(22 hours);
        actual = limiter.calculateWindowAmount(supportedChainID);
        assertEq(actual, 2 * USD_VALUE_MULTIPLIER);
        skip(59 minutes);
        actual = limiter.calculateWindowAmount(supportedChainID);
        assertEq(actual, 2 * USD_VALUE_MULTIPLIER);
        skip(1 minutes);
        actual = limiter.calculateWindowAmount(supportedChainID);
        assertEq(actual, 0);
    }

    function testAmountWillExceedLimit() public {
        changePrank(address(bridge));
        uint8 tokenID = 3;
        uint256 amount = 999999 * 1000000; // USDC has 6 decimals
        assertFalse(limiter.willAmountExceedLimit(supportedChainID, tokenID, amount));
        limiter.recordBridgeTransfers(supportedChainID, tokenID, amount);
        assertTrue(limiter.willAmountExceedLimit(supportedChainID, tokenID, 2000000));
        assertFalse(limiter.willAmountExceedLimit(supportedChainID, tokenID, 1000000));
    }

    function testRecordBridgeTransfer() public {
        changePrank(address(bridge));
        uint8 tokenID = 1;
        uint256 amount = 1_00000000; // wBTC has 8 decimals
        limiter.recordBridgeTransfers(supportedChainID, tokenID, amount);
        tokenID = 2;
        amount = 1 ether;
        limiter.recordBridgeTransfers(supportedChainID, tokenID, amount);
        tokenID = 3;
        amount = 1000000; // USDC has 6 decimals
        limiter.recordBridgeTransfers(supportedChainID, tokenID, amount);
        uint256 key =
            limiter.getChainHourTimestampKey(supportedChainID, uint32(block.timestamp / 1 hours));
        assertEq(limiter.chainHourlyTransferAmount(key), BTC_PRICE + ETH_PRICE + USDC_PRICE);
    }

    function testrecordBridgeTransfersGarbageCollection() public {
        changePrank(address(bridge));
        uint8 tokenID = 1;
        uint256 amount = 1_00000000; // wBTC has 8 decimals
        uint32 hourToDelete = uint32(block.timestamp / 1 hours);
        limiter.recordBridgeTransfers(supportedChainID, tokenID, amount);
        uint256 keyToDelete = limiter.getChainHourTimestampKey(supportedChainID, hourToDelete);
        uint256 deleteAmount = limiter.chainHourlyTransferAmount(keyToDelete);
        assertEq(deleteAmount, BTC_PRICE);
        skip(25 hours);
        limiter.recordBridgeTransfers(supportedChainID, tokenID, amount);
        deleteAmount = limiter.chainHourlyTransferAmount(keyToDelete);
        assertEq(deleteAmount, 0);
    }

    function testUpdateLimitWithSignatures() public {
        changePrank(address(bridge));
        uint8 sourceChainID = 0;
        uint64 newLimit = 10 * USD_VALUE_MULTIPLIER;
        bytes memory payload = abi.encodePacked(sourceChainID, newLimit);
        // Create a sample BridgeUtils
        BridgeUtils.Message memory message = BridgeUtils.Message({
            messageType: BridgeUtils.UPDATE_BRIDGE_LIMIT,
            version: 1,
            nonce: 0,
            chainID: chainID,
            payload: payload
        });

        bytes memory messageBytes = BridgeUtils.encodeMessage(message);
        bytes32 messageHash = keccak256(messageBytes);

        bytes[] memory signatures = new bytes[](4);
        signatures[0] = getSignature(messageHash, committeeMemberPkA);
        signatures[1] = getSignature(messageHash, committeeMemberPkB);
        signatures[2] = getSignature(messageHash, committeeMemberPkC);
        signatures[3] = getSignature(messageHash, committeeMemberPkD);

        assertEq(limiter.chainLimits(supportedChainID), totalLimit);

        // Call the updateLimitWithSignatures function
        limiter.updateLimitWithSignatures(signatures, message);

        assertEq(limiter.chainLimits(supportedChainID), 10 * USD_VALUE_MULTIPLIER);
    }

    function testMultipleChainLimits() public {
        // deploy new committee
        address[] memory _committeeList = new address[](5);
        uint16[] memory _stake = new uint16[](5);
        _committeeList[0] = committeeMemberA;
        _committeeList[1] = committeeMemberB;
        _committeeList[2] = committeeMemberC;
        _committeeList[3] = committeeMemberD;
        _committeeList[4] = committeeMemberE;
        _stake[0] = 1000;
        _stake[1] = 1000;
        _stake[2] = 1000;
        _stake[3] = 2002;
        _stake[4] = 4998;
        address _committee = Upgrades.deployUUPSProxy(
            "BridgeCommittee.sol",
            abi.encodeCall(BridgeCommittee.initialize, (_committeeList, _stake, minStakeRequired)),
            opts
        );

        committee = BridgeCommittee(_committee);

        // deploy new config contract with 2 supported chains
        address[] memory _supportedTokens = new address[](5);
        _supportedTokens[0] = address(0); // SUI
        _supportedTokens[1] = wBTC;
        _supportedTokens[2] = wETH;
        _supportedTokens[3] = USDC;
        _supportedTokens[4] = USDT;
        uint8[] memory supportedChains = new uint8[](2);
        supportedChains[0] = 11;
        supportedChains[1] = 12;
        address _config = Upgrades.deployUUPSProxy(
            "BridgeConfig.sol",
            abi.encodeCall(
                BridgeConfig.initialize,
                (address(committee), chainID, _supportedTokens, tokenPrices, tokenIds, suiDecimals, supportedChains)
            ),
            opts
        );
        committee.initializeConfig(_config);
        // deploy new limiter with 2 supported chains
        uint64[] memory totalLimits = new uint64[](2);
        totalLimits[0] = 1_000_000 * USD_VALUE_MULTIPLIER;
        totalLimits[1] = 2_000_000 * USD_VALUE_MULTIPLIER;
        address _limiter = Upgrades.deployUUPSProxy(
            "BridgeLimiter.sol",
            abi.encodeCall(
                BridgeLimiter.initialize, (address(committee), supportedChains, totalLimits)
            ),
            opts
        );
        limiter = BridgeLimiter(_limiter);
        // check if the limits are set correctly
        assertEq(limiter.chainLimits(11), 1_000_000 * USD_VALUE_MULTIPLIER);
        assertEq(limiter.chainLimits(12), 2_000_000 * USD_VALUE_MULTIPLIER);
        // check if the oldestChainTimestamp is set correctly
        assertEq(limiter.oldestChainTimestamp(11), uint32(block.timestamp / 1 hours));
        assertEq(limiter.oldestChainTimestamp(12), uint32(block.timestamp / 1 hours));

        // check that limits are checked correctly
        uint8 tokenID = 3;
        uint256 amount = 999_999 * 1000000; // USDC has 6 decimals
        assertFalse(
            limiter.willAmountExceedLimit(11, tokenID, amount), "limit should not be exceeded"
        );
        limiter.recordBridgeTransfers(11, tokenID, amount);

        assertTrue(limiter.willAmountExceedLimit(11, tokenID, 2000000), "limit should be exceeded");
        assertFalse(
            limiter.willAmountExceedLimit(11, tokenID, 1000000), "limit should not be exceeded"
        );
        assertEq(
            limiter.calculateWindowAmount(11),
            999999 * USD_VALUE_MULTIPLIER,
            "window amount should be correct"
        );
        assertEq(limiter.calculateWindowAmount(12), 0, "window amount should be correct");
        // check that transfers are recorded correctly
        amount = 1100000 * 1000000; // USDC has 6 decimals
        limiter.recordBridgeTransfers(12, tokenID, amount);
        assertEq(
            limiter.chainHourlyTransferAmount(
                limiter.getChainHourTimestampKey(12, uint32(block.timestamp / 1 hours))
            ),
            1_100_000 * USD_VALUE_MULTIPLIER,
            "transfer amount should be correct"
        );
        assertEq(
            limiter.calculateWindowAmount(11),
            999999 * USD_VALUE_MULTIPLIER,
            "window amount should be correct"
        );
        assertEq(
            limiter.calculateWindowAmount(12),
            1100000 * USD_VALUE_MULTIPLIER,
            "window amount should be correct"
        );
    }

    // An e2e update limit regression test covering message ser/de
    function testUpdateLimitRegressionTest() public {
        address[] memory _committeeList = new address[](4);
        uint16[] memory _stake = new uint16[](4);
        uint8 chainID = 11;
        uint8[] memory _supportedChains = new uint8[](1);
        uint8 sendingChainID = 1;
        _supportedChains[0] = sendingChainID;
        uint8[] memory _supportedDestinationChains = new uint8[](1);
        _supportedDestinationChains[0] = sendingChainID;
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

        // deploy config
        tokenPrices = new uint64[](5);
        tokenPrices[0] = 10000; // SUI PRICE
        tokenPrices[1] = 10000; // BTC PRICE
        tokenPrices[2] = 10000; // ETH PRICE
        tokenPrices[3] = 10000; // USDC PRICE
        tokenPrices[4] = 10000; // USDT PRICE
        address _config = Upgrades.deployUUPSProxy(
            "BridgeConfig.sol",
            abi.encodeCall(
                BridgeConfig.initialize,
                (address(committee), chainID, supportedTokens, tokenPrices, tokenIds, suiDecimals, _supportedChains)
            ),
            opts
        );

        // initialize config in the bridge committee
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

        bytes memory payload = hex"0c00000002540be400";

        // Create update bridge limit message
        BridgeUtils.Message memory message = BridgeUtils.Message({
            messageType: BridgeUtils.UPDATE_BRIDGE_LIMIT,
            version: 1,
            nonce: 15,
            chainID: 2,
            payload: payload
        });
        bytes memory encodedMessage = BridgeUtils.encodeMessage(message);
        bytes memory expectedEncodedMessage =
            hex"5355495f4252494447455f4d4553534147450301000000000000000f020c00000002540be400";

        assertEq(encodedMessage, expectedEncodedMessage);
    }

    // An e2e update limit regression test covering message ser/de and signature verification
    function testUpdateLimitRegressionTestWithSigVerficiation() public {
        address[] memory _committeeList = new address[](4);
        uint16[] memory _stake = new uint16[](4);
        uint8 chainID = 11;
        uint8[] memory _supportedChains = new uint8[](1);
        uint8 sendingChainID = 1;
        _supportedChains[0] = sendingChainID;
        uint8[] memory _supportedDestinationChains = new uint8[](1);
        _supportedDestinationChains[0] = sendingChainID;
        _committeeList[0] = 0x68B43fD906C0B8F024a18C56e06744F7c6157c65;
        _committeeList[1] = 0xaCAEf39832CB995c4E049437A3E2eC6a7bad1Ab5;
        _committeeList[2] = 0x8061f127910e8eF56F16a2C411220BaD25D61444;
        _committeeList[3] = 0x508F3F1ff45F4ca3D8e86CDCC91445F00aCC59fC;
        _stake[0] = 2500;
        _stake[1] = 2500;
        _stake[2] = 2500;
        _stake[3] = 2500;
        committee = new BridgeCommittee();
        address _committee = Upgrades.deployUUPSProxy(
            "BridgeCommittee.sol",
            abi.encodeCall(BridgeCommittee.initialize, (_committeeList, _stake, minStakeRequired)),
            opts
        );

        committee = BridgeCommittee(_committee);

        // deploy config
        tokenPrices = new uint64[](5);
        tokenPrices[0] = 10000; // SUI PRICE
        tokenPrices[1] = 10000; // BTC PRICE
        tokenPrices[2] = 10000; // ETH PRICE
        tokenPrices[3] = 10000; // USDC PRICE
        tokenPrices[4] = 10000; // USDT PRICE
        address _config = Upgrades.deployUUPSProxy(
            "BridgeConfig.sol",
            abi.encodeCall(
                BridgeConfig.initialize,
                (address(committee), chainID, supportedTokens, tokenPrices, tokenIds, suiDecimals, _supportedChains)
            ),
            opts
        );

        // initialize config in the bridge committee
        committee.initializeConfig(_config);

        vault = new BridgeVault(wETH);

        skip(2 days);

        uint64[] memory totalLimits = new uint64[](1);
        totalLimits[0] = 100 * USD_VALUE_MULTIPLIER;

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

        vault.transferOwnership(address(bridge));
        limiter.transferOwnership(address(bridge));

        // Fill vault with WETH
        changePrank(deployer);
        IWETH9(wETH).deposit{value: 10 ether}();
        IERC20(wETH).transfer(address(vault), 10 ether);
        // sending chain: 01 (sendingChainID), new limit: 99_900 * USD_VALUE_MULTIPLIER
        bytes memory payload = hex"0100000915fa66bc00";

        // Create update bridge limit message
        BridgeUtils.Message memory message = BridgeUtils.Message({
            messageType: BridgeUtils.UPDATE_BRIDGE_LIMIT,
            version: 1,
            nonce: 0,
            chainID: chainID,
            payload: payload
        });
        bytes memory encodedMessage = BridgeUtils.encodeMessage(message);
        bytes memory expectedEncodedMessage =
            hex"5355495f4252494447455f4d455353414745030100000000000000000b0100000915fa66bc00";

        assertEq(encodedMessage, expectedEncodedMessage);

        bytes[] memory signatures = new bytes[](3);

        signatures[0] =
            hex"d19f71162a73150af2b786befbf248914bd421ac42a7345c47b2ef48f98d24a45eba60676ea17c6aa25ca0f548d3ef97e1498b7232576f08b91b09e1a8daeec001";
        signatures[1] =
            hex"5f9de5595ea57405c8b4e2728864f6fd33399f2bb22c5b9e24ee36f9a357d61223512a20cce8eb536c10e99c21c35f357ae26a5cb2083c495d8f280b31d89ec300";
        signatures[2] =
            hex"33deda897325e500e84ab97eac33cc3d5bdc3ec46361ab8df1068da71bd8bf077f0f1265d80b23c590c55eeee2612d4cfae03a21f7c67e268c4f09dc2f1a0d9401";

        committee.verifySignatures(signatures, message);

        limiter.updateLimitWithSignatures(signatures, message);
        assertEq(limiter.chainLimits(sendingChainID), 99_900 * USD_VALUE_MULTIPLIER);
    }
}
