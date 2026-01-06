// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "./BridgeBaseTest.t.sol";
import "@openzeppelin/contracts/token/ERC20/extensions/IERC20Metadata.sol";
import "../contracts/SuiBridgeV2.sol";
import "../contracts/utils/BridgeUtilsV2.sol";

contract SuiBridgeV2Test is BridgeBaseTest {
    SuiBridgeV2 bridgeV2;
    uint8 _chainID = 12;

    // V2 event declaration for testing
    event TokensDepositedV2(
        uint8 indexed sourceChainID,
        uint64 indexed nonce,
        uint8 indexed destinationChainID,
        uint8 tokenID,
        uint64 suiAdjustedAmount,
        address senderAddress,
        bytes recipientAddress,
        uint256 timestampMs
    );

    // This function is called before each unit test
    function setUp() public {
        setUpBridgeTest();
        _deployBridgeV2();
    }

    function _deployBridgeV2() internal {
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
                (
                    _committee,
                    _chainID,
                    supportedTokens,
                    tokenPrices,
                    tokenIds,
                    suiDecimals,
                    _supportedDestinationChains
                )
            ),
            opts
        );

        committee.initializeConfig(_config);

        // deploy vault
        vault = new BridgeVault(wETH);

        // deploy limiter
        uint64[] memory chainLimits = new uint64[](1);
        chainLimits[0] = totalLimit;

        address _limiter = Upgrades.deployUUPSProxy(
            "BridgeLimiter.sol",
            abi.encodeCall(
                BridgeLimiter.initialize,
                (address(committee), _supportedDestinationChains, chainLimits)
            ),
            opts
        );

        limiter = BridgeLimiter(_limiter);

        // deploy SuiBridgeV2 directly (not upgrading from V1)
        address _bridgeV2 = Upgrades.deployUUPSProxy(
            "SuiBridgeV2.sol",
            abi.encodeCall(
                SuiBridge.initialize, (address(committee), address(vault), address(limiter))
            ),
            opts
        );

        bridgeV2 = SuiBridgeV2(_bridgeV2);

        vault.transferOwnership(address(bridgeV2));
        limiter.transferOwnership(address(bridgeV2));
    }

    /* ========== BRIDGE ETH V2 TESTS ========== */

    function testBridgeETHV2() public {
        changePrank(deployer);
        assertEq(IERC20(wETH).balanceOf(address(vault)), 0);
        uint256 balance = deployer.balance;

        // assert emitted event with timestamp
        vm.expectEmit(true, true, true, true);
        emit TokensDepositedV2(
            _chainID,
            0, // nonce
            0, // destination chain id
            BridgeUtils.ETH,
            1_00_000_000, // 1 ether in sui decimals
            deployer,
            hex"06bb77410cd326430fa2036c8282dbb54a6f8640cea16ef5eff32d638718b3e4",
            block.timestamp
        );

        bridgeV2.bridgeETHV2{value: 1 ether}(
            hex"06bb77410cd326430fa2036c8282dbb54a6f8640cea16ef5eff32d638718b3e4", 0
        );

        assertEq(IERC20(wETH).balanceOf(address(vault)), 1 ether);
        assertEq(deployer.balance, balance - 1 ether);
        assertEq(bridgeV2.nonces(BridgeUtils.TOKEN_TRANSFER), 1);
    }

    function testBridgeETHV2InvalidRecipientAddress() public {
        vm.expectRevert(bytes("SuiBridge: Invalid recipient address length"));
        bridgeV2.bridgeETHV2{value: 1 ether}(
            hex"06bb77410cd326430fa2036c8282dbb54a6f8640cea16ef5eff32d638718b3", 0
        );
    }

    function testBridgeETHV2WhenPaused() public {
        // Pause the bridge
        _pauseBridge();

        vm.expectRevert();
        bridgeV2.bridgeETHV2{value: 1 ether}(
            hex"06bb77410cd326430fa2036c8282dbb54a6f8640cea16ef5eff32d638718b3e4", 0
        );
    }

    /* ========== BRIDGE ERC20 V2 TESTS ========== */

    function testBridgeWETHV2() public {
        changePrank(deployer);
        IWETH9(wETH).deposit{value: 10 ether}();
        IERC20(wETH).approve(address(bridgeV2), 10 ether);
        assertEq(IERC20(wETH).balanceOf(address(vault)), 0);
        uint256 balance = IERC20(wETH).balanceOf(deployer);

        // assert emitted event with timestamp
        vm.expectEmit(true, true, true, true);
        emit TokensDepositedV2(
            _chainID,
            0, // nonce
            0, // destination chain id
            BridgeUtils.ETH,
            1_00_000_000, // 1 ether in sui decimals
            deployer,
            hex"06bb77410cd326430fa2036c8282dbb54a6f8640cea16ef5eff32d638718b3e4",
            block.timestamp
        );

        bridgeV2.bridgeERC20V2(
            BridgeUtils.ETH,
            1 ether,
            hex"06bb77410cd326430fa2036c8282dbb54a6f8640cea16ef5eff32d638718b3e4",
            0
        );
        assertEq(IERC20(wETH).balanceOf(address(vault)), 1 ether);
        assertEq(IERC20(wETH).balanceOf(deployer), balance - 1 ether);
        assertEq(bridgeV2.nonces(BridgeUtils.TOKEN_TRANSFER), 1);
    }

    function testBridgeUSDCV2() public {
        changePrank(USDCWhale);

        uint256 usdcAmount = 1000000;

        // approve
        IERC20(USDC).approve(address(bridgeV2), usdcAmount);

        assertEq(IERC20(USDC).balanceOf(address(vault)), 0);
        uint256 balance = IERC20(USDC).balanceOf(USDCWhale);

        // assert emitted event with timestamp
        vm.expectEmit(true, true, true, true);
        emit TokensDepositedV2(
            _chainID,
            0, // nonce
            0, // destination chain id
            BridgeUtils.USDC,
            1_000_000, // 1 USDC
            USDCWhale,
            hex"06bb77410cd326430fa2036c8282dbb54a6f8640cea16ef5eff32d638718b3e4",
            block.timestamp
        );

        bridgeV2.bridgeERC20V2(
            BridgeUtils.USDC,
            usdcAmount,
            hex"06bb77410cd326430fa2036c8282dbb54a6f8640cea16ef5eff32d638718b3e4",
            0
        );

        assertEq(IERC20(USDC).balanceOf(USDCWhale), balance - usdcAmount);
        assertEq(IERC20(USDC).balanceOf(address(vault)), usdcAmount);
    }

    function testBridgeERC20V2UnsupportedToken() public {
        vm.expectRevert(bytes("SuiBridge: Unsupported token"));
        bridgeV2.bridgeERC20V2(
            255, 1 ether, hex"06bb77410cd326430fa2036c8282dbb54a6f8640cea16ef5eff32d638718b3e4", 0
        );
    }

    function testBridgeERC20V2InvalidRecipientAddress() public {
        vm.expectRevert(bytes("SuiBridge: Invalid recipient address length"));
        bridgeV2.bridgeERC20V2(
            BridgeUtils.ETH,
            1 ether,
            hex"06bb77410cd326430fa2036c8282dbb54a6f8640cea16ef5eff32d638718b3",
            0
        );
    }

    function testBridgeERC20V2InsufficientAllowance() public {
        vm.expectRevert(bytes("SuiBridge: Insufficient allowance"));
        bridgeV2.bridgeERC20V2(
            BridgeUtils.ETH,
            type(uint256).max,
            hex"06bb77410cd326430fa2036c8282dbb54a6f8640cea16ef5eff32d638718b3e4",
            0
        );
    }

    /* ========== TRANSFER BRIDGED TOKENS V2 TESTS ========== */

    function testTransferBridgedTokensWithSignaturesV2() public {
        // Fill vault with WETH
        changePrank(deployer);
        IWETH9(wETH).deposit{value: 10 ether}();
        IERC20(wETH).transfer(address(vault), 10 ether);

        // Create V2 transfer payload (71 bytes)
        bytes memory payload = _createV2TransferPayload(
            bridgerA,
            BridgeUtils.ETH,
            100000000,
            block.timestamp // 1 ether in sui decimals
        );

        // Create transfer message with version 2
        BridgeUtils.Message memory message = BridgeUtils.Message({
            messageType: BridgeUtils.TOKEN_TRANSFER,
            version: 2,
            nonce: 1,
            chainID: 0, // sending chain
            payload: payload
        });

        bytes memory encodedMessage = BridgeUtils.encodeMessage(message);
        bytes32 messageHash = keccak256(encodedMessage);

        bytes[] memory signatures = new bytes[](4);
        signatures[0] = getSignature(messageHash, committeeMemberPkA);
        signatures[1] = getSignature(messageHash, committeeMemberPkB);
        signatures[2] = getSignature(messageHash, committeeMemberPkC);
        signatures[3] = getSignature(messageHash, committeeMemberPkD);

        uint256 aBalance = bridgerA.balance;
        bridgeV2.transferBridgedTokensWithSignaturesV2(signatures, message);
        assertEq(bridgerA.balance, aBalance + 1 ether);
    }

    function testTransferBridgedTokensV2InvalidVersion() public {
        // Fill vault with WETH
        changePrank(deployer);
        IWETH9(wETH).deposit{value: 10 ether}();
        IERC20(wETH).transfer(address(vault), 10 ether);

        // Create V2 payload but with version 1 message
        bytes memory payload =
            _createV2TransferPayload(bridgerA, BridgeUtils.ETH, 100000000, block.timestamp);

        BridgeUtils.Message memory message = BridgeUtils.Message({
            messageType: BridgeUtils.TOKEN_TRANSFER,
            version: 1, // Wrong version!
            nonce: 1,
            chainID: 0,
            payload: payload
        });

        bytes memory encodedMessage = BridgeUtils.encodeMessage(message);
        bytes32 messageHash = keccak256(encodedMessage);

        bytes[] memory signatures = new bytes[](4);
        signatures[0] = getSignature(messageHash, committeeMemberPkA);
        signatures[1] = getSignature(messageHash, committeeMemberPkB);
        signatures[2] = getSignature(messageHash, committeeMemberPkC);
        signatures[3] = getSignature(messageHash, committeeMemberPkD);

        vm.expectRevert(bytes("SuiBridge: Invalid message version"));
        bridgeV2.transferBridgedTokensWithSignaturesV2(signatures, message);
    }

    function testTransferBridgedTokensV2MessageAlreadyProcessed() public {
        // Fill vault with WETH
        changePrank(deployer);
        IWETH9(wETH).deposit{value: 10 ether}();
        IERC20(wETH).transfer(address(vault), 10 ether);

        bytes memory payload =
            _createV2TransferPayload(bridgerA, BridgeUtils.ETH, 100000000, block.timestamp);

        BridgeUtils.Message memory message = BridgeUtils.Message({
            messageType: BridgeUtils.TOKEN_TRANSFER,
            version: 2,
            nonce: 1,
            chainID: 0,
            payload: payload
        });

        bytes memory encodedMessage = BridgeUtils.encodeMessage(message);
        bytes32 messageHash = keccak256(encodedMessage);

        bytes[] memory signatures = new bytes[](4);
        signatures[0] = getSignature(messageHash, committeeMemberPkA);
        signatures[1] = getSignature(messageHash, committeeMemberPkB);
        signatures[2] = getSignature(messageHash, committeeMemberPkC);
        signatures[3] = getSignature(messageHash, committeeMemberPkD);

        // First transfer succeeds
        bridgeV2.transferBridgedTokensWithSignaturesV2(signatures, message);

        // Second transfer with same nonce should fail
        vm.expectRevert(bytes("SuiBridge: Message already processed"));
        bridgeV2.transferBridgedTokensWithSignaturesV2(signatures, message);
    }

    /* ========== MATURE MESSAGE (48H LIMITER BYPASS) TESTS ========== */

    function testMatureMessageBypassesLimit() public {
        // Fill vault with WETH - need enough for transfer
        changePrank(deployer);
        vm.deal(deployer, 600 ether);
        IWETH9(wETH).deposit{value: 500 ether}();
        IERC20(wETH).transfer(address(vault), 500 ether);

        // Create a transfer that would exceed the limit if it were fresh
        // totalLimit is 1_000_000 USD, ETH price ~$2597, so ~386 ETH exceeds limit
        // Use 100 ETH which exceeds limit but we have funds for
        uint64 largeAmount = 100_00000000; // 100 ETH in sui decimals

        // Create message with timestamp from 49 hours ago (mature)
        uint256 matureTimestamp = block.timestamp - (49 * 3600);
        bytes memory payload =
            _createV2TransferPayload(bridgerA, BridgeUtils.ETH, largeAmount, matureTimestamp);

        BridgeUtils.Message memory message = BridgeUtils.Message({
            messageType: BridgeUtils.TOKEN_TRANSFER,
            version: 2,
            nonce: 1,
            chainID: 0,
            payload: payload
        });

        bytes memory encodedMessage = BridgeUtils.encodeMessage(message);
        bytes32 messageHash = keccak256(encodedMessage);

        bytes[] memory signatures = new bytes[](4);
        signatures[0] = getSignature(messageHash, committeeMemberPkA);
        signatures[1] = getSignature(messageHash, committeeMemberPkB);
        signatures[2] = getSignature(messageHash, committeeMemberPkC);
        signatures[3] = getSignature(messageHash, committeeMemberPkD);

        // This should succeed because the message is mature (>48h old)
        uint256 aBalance = bridgerA.balance;
        bridgeV2.transferBridgedTokensWithSignaturesV2(signatures, message);
        assertEq(bridgerA.balance, aBalance + 100 ether);
    }

    function testFreshMessageRespectsLimit() public {
        // Fill vault with WETH
        changePrank(deployer);
        vm.deal(deployer, 600 ether);
        IWETH9(wETH).deposit{value: 500 ether}();
        IERC20(wETH).transfer(address(vault), 500 ether);

        // Create a transfer that exceeds the limit
        // totalLimit is 1_000_000 USD, ETH price ~$2597, so ~386 ETH exceeds limit
        uint64 largeAmount = 400_00000000; // 400 ETH in sui decimals - exceeds $1M limit

        // Create message with current timestamp (fresh, not mature)
        bytes memory payload =
            _createV2TransferPayload(bridgerA, BridgeUtils.ETH, largeAmount, block.timestamp * 1000);

        BridgeUtils.Message memory message = BridgeUtils.Message({
            messageType: BridgeUtils.TOKEN_TRANSFER,
            version: 2,
            nonce: 1,
            chainID: 0,
            payload: payload
        });

        bytes memory encodedMessage = BridgeUtils.encodeMessage(message);
        bytes32 messageHash = keccak256(encodedMessage);

        bytes[] memory signatures = new bytes[](4);
        signatures[0] = getSignature(messageHash, committeeMemberPkA);
        signatures[1] = getSignature(messageHash, committeeMemberPkB);
        signatures[2] = getSignature(messageHash, committeeMemberPkC);
        signatures[3] = getSignature(messageHash, committeeMemberPkD);

        // This should fail because the message is fresh and exceeds limit
        vm.expectRevert(bytes("SuiBridge: Amount exceeds bridge limit"));
        bridgeV2.transferBridgedTokensWithSignaturesV2(signatures, message);
    }

    function testMessageAt48HourBoundary() public {
        // Fill vault with WETH
        changePrank(deployer);
        vm.deal(deployer, 600 ether);
        IWETH9(wETH).deposit{value: 500 ether}();
        IERC20(wETH).transfer(address(vault), 500 ether);

        uint64 largeAmount = 400_00000000; // 400 ETH - exceeds limit

        // Create message with timestamp exactly 48 hours ago (not mature yet)
        uint256 boundaryTimestamp = (block.timestamp - (48 * 3600)) * 1000;
        bytes memory payload =
            _createV2TransferPayload(bridgerA, BridgeUtils.ETH, largeAmount, boundaryTimestamp);

        BridgeUtils.Message memory message = BridgeUtils.Message({
            messageType: BridgeUtils.TOKEN_TRANSFER,
            version: 2,
            nonce: 1,
            chainID: 0,
            payload: payload
        });

        bytes memory encodedMessage = BridgeUtils.encodeMessage(message);
        bytes32 messageHash = keccak256(encodedMessage);

        bytes[] memory signatures = new bytes[](4);
        signatures[0] = getSignature(messageHash, committeeMemberPkA);
        signatures[1] = getSignature(messageHash, committeeMemberPkB);
        signatures[2] = getSignature(messageHash, committeeMemberPkC);
        signatures[3] = getSignature(messageHash, committeeMemberPkD);

        // At exactly 48h boundary, message is NOT mature (needs to be > 48h)
        vm.expectRevert(bytes("SuiBridge: Amount exceeds bridge limit"));
        bridgeV2.transferBridgedTokensWithSignaturesV2(signatures, message);
    }

    function testMatureUSDCTransferBypassesLimit() public {
        // Fund vault with USDC from whale
        changePrank(USDCWhale);
        IERC20(USDC).transfer(address(vault), 2_000_000 * 1e6); // $2M USDC

        // Create a large USDC transfer that would exceed limit if fresh
        // totalLimit is $1M, so $1.5M should exceed it
        uint64 largeAmount = 1_500_000_000000; // $1.5M USDC in sui decimals (6 decimals)

        // Create message with timestamp from 49 hours ago (mature)
        uint256 matureTimestamp = block.timestamp - (49 * 3600);
        bytes memory payload =
            _createV2TransferPayload(bridgerA, BridgeUtils.USDC, largeAmount, matureTimestamp);

        BridgeUtils.Message memory message = BridgeUtils.Message({
            messageType: BridgeUtils.TOKEN_TRANSFER,
            version: 2,
            nonce: 1,
            chainID: 0,
            payload: payload
        });

        bytes memory encodedMessage = BridgeUtils.encodeMessage(message);
        bytes32 messageHash = keccak256(encodedMessage);

        bytes[] memory signatures = new bytes[](4);
        signatures[0] = getSignature(messageHash, committeeMemberPkA);
        signatures[1] = getSignature(messageHash, committeeMemberPkB);
        signatures[2] = getSignature(messageHash, committeeMemberPkC);
        signatures[3] = getSignature(messageHash, committeeMemberPkD);

        // This should succeed because the message is mature (>48h old)
        uint256 aBalanceBefore = IERC20(USDC).balanceOf(bridgerA);
        bridgeV2.transferBridgedTokensWithSignaturesV2(signatures, message);
        assertEq(IERC20(USDC).balanceOf(bridgerA), aBalanceBefore + 1_500_000 * 1e6);
    }

    function testMatureMessageDoesNotUpdateLimiter() public {
        // Fill vault with WETH
        changePrank(deployer);
        vm.deal(deployer, 200 ether);
        IWETH9(wETH).deposit{value: 100 ether}();
        IERC20(wETH).transfer(address(vault), 100 ether);

        // Record limiter window amount before (chainID = 0 for source chain)
        uint256 windowAmountBefore = limiter.calculateWindowAmount(0);

        // Create mature message (49h old) - should NOT update limiter
        uint64 amount = 10_00000000; // 10 ETH
        uint256 matureTimestamp = block.timestamp - (49 * 3600);
        bytes memory payload =
            _createV2TransferPayload(bridgerA, BridgeUtils.ETH, amount, matureTimestamp);

        BridgeUtils.Message memory message = BridgeUtils.Message({
            messageType: BridgeUtils.TOKEN_TRANSFER,
            version: 2,
            nonce: 1,
            chainID: 0,
            payload: payload
        });

        bytes memory encodedMessage = BridgeUtils.encodeMessage(message);
        bytes32 messageHash = keccak256(encodedMessage);

        bytes[] memory signatures = new bytes[](4);
        signatures[0] = getSignature(messageHash, committeeMemberPkA);
        signatures[1] = getSignature(messageHash, committeeMemberPkB);
        signatures[2] = getSignature(messageHash, committeeMemberPkC);
        signatures[3] = getSignature(messageHash, committeeMemberPkD);

        bridgeV2.transferBridgedTokensWithSignaturesV2(signatures, message);

        // Limiter should NOT be updated for mature messages
        assertEq(limiter.calculateWindowAmount(0), windowAmountBefore);
    }

    function testFreshMessageUpdatesLimiter() public {
        // Fill vault with WETH
        changePrank(deployer);
        vm.deal(deployer, 20 ether);
        IWETH9(wETH).deposit{value: 10 ether}();
        IERC20(wETH).transfer(address(vault), 10 ether);

        // Record limiter window amount before
        uint256 windowAmountBefore = limiter.calculateWindowAmount(0);

        // Create fresh message - should update limiter
        uint64 amount = 1_00000000; // 1 ETH (under limit)
        bytes memory payload =
            _createV2TransferPayload(bridgerA, BridgeUtils.ETH, amount, block.timestamp * 1000);

        BridgeUtils.Message memory message = BridgeUtils.Message({
            messageType: BridgeUtils.TOKEN_TRANSFER,
            version: 2,
            nonce: 1,
            chainID: 0,
            payload: payload
        });

        bytes memory encodedMessage = BridgeUtils.encodeMessage(message);
        bytes32 messageHash = keccak256(encodedMessage);

        bytes[] memory signatures = new bytes[](4);
        signatures[0] = getSignature(messageHash, committeeMemberPkA);
        signatures[1] = getSignature(messageHash, committeeMemberPkB);
        signatures[2] = getSignature(messageHash, committeeMemberPkC);
        signatures[3] = getSignature(messageHash, committeeMemberPkD);

        bridgeV2.transferBridgedTokensWithSignaturesV2(signatures, message);

        // Limiter SHOULD be updated for fresh messages
        assertGt(limiter.calculateWindowAmount(0), windowAmountBefore);
    }

    function testVeryOldMatureMessageStillBypasses() public {
        // Fill vault with WETH
        changePrank(deployer);
        vm.deal(deployer, 200 ether);
        IWETH9(wETH).deposit{value: 100 ether}();
        IERC20(wETH).transfer(address(vault), 100 ether);

        // Create a transfer that would exceed limit if fresh
        uint64 largeAmount = 50_00000000; // 50 ETH

        // Create message from 1 week ago (very mature)
        uint256 veryOldTimestamp = block.timestamp - (7 * 24 * 3600);
        bytes memory payload =
            _createV2TransferPayload(bridgerA, BridgeUtils.ETH, largeAmount, veryOldTimestamp);

        BridgeUtils.Message memory message = BridgeUtils.Message({
            messageType: BridgeUtils.TOKEN_TRANSFER,
            version: 2,
            nonce: 1,
            chainID: 0,
            payload: payload
        });

        bytes memory encodedMessage = BridgeUtils.encodeMessage(message);
        bytes32 messageHash = keccak256(encodedMessage);

        bytes[] memory signatures = new bytes[](4);
        signatures[0] = getSignature(messageHash, committeeMemberPkA);
        signatures[1] = getSignature(messageHash, committeeMemberPkB);
        signatures[2] = getSignature(messageHash, committeeMemberPkC);
        signatures[3] = getSignature(messageHash, committeeMemberPkD);

        // Very old messages should also bypass the limiter
        uint256 aBalance = bridgerA.balance;
        bridgeV2.transferBridgedTokensWithSignaturesV2(signatures, message);
        assertEq(bridgerA.balance, aBalance + 50 ether);
    }

    function testMatureMessageJustOver48Hours() public {
        // Fill vault with WETH
        changePrank(deployer);
        vm.deal(deployer, 200 ether);
        IWETH9(wETH).deposit{value: 100 ether}();
        IERC20(wETH).transfer(address(vault), 100 ether);

        // Create a transfer that would exceed limit if fresh
        uint64 largeAmount = 50_00000000; // 50 ETH

        // Create message just 1 second over 48 hours (should be mature)
        uint256 justMatureTimestamp = block.timestamp - (48 * 3600 + 1);
        bytes memory payload =
            _createV2TransferPayload(bridgerA, BridgeUtils.ETH, largeAmount, justMatureTimestamp);

        BridgeUtils.Message memory message = BridgeUtils.Message({
            messageType: BridgeUtils.TOKEN_TRANSFER,
            version: 2,
            nonce: 1,
            chainID: 0,
            payload: payload
        });

        bytes memory encodedMessage = BridgeUtils.encodeMessage(message);
        bytes32 messageHash = keccak256(encodedMessage);

        bytes[] memory signatures = new bytes[](4);
        signatures[0] = getSignature(messageHash, committeeMemberPkA);
        signatures[1] = getSignature(messageHash, committeeMemberPkB);
        signatures[2] = getSignature(messageHash, committeeMemberPkC);
        signatures[3] = getSignature(messageHash, committeeMemberPkD);

        // Just over 48h should succeed
        uint256 aBalance = bridgerA.balance;
        bridgeV2.transferBridgedTokensWithSignaturesV2(signatures, message);
        assertEq(bridgerA.balance, aBalance + 50 ether);
    }

    /* ========== V1 FUNCTIONS STILL WORK TESTS ========== */

    function testV1FunctionsStillWorkOnV2() public {
        // Test that V1 bridgeETH still works
        changePrank(deployer);
        assertEq(IERC20(wETH).balanceOf(address(vault)), 0);

        bridgeV2.bridgeETH{value: 1 ether}(
            hex"06bb77410cd326430fa2036c8282dbb54a6f8640cea16ef5eff32d638718b3e4", 0
        );

        assertEq(IERC20(wETH).balanceOf(address(vault)), 1 ether);
    }

    function testV1TransferStillWorksOnV2() public {
        // Fill vault with WETH
        changePrank(deployer);
        IWETH9(wETH).deposit{value: 10 ether}();
        IERC20(wETH).transfer(address(vault), 10 ether);

        // Create V1 transfer payload (64 bytes, no timestamp)
        uint8 senderAddressLength = 32;
        bytes memory senderAddress = abi.encode(0);
        uint8 targetChain = _chainID;
        uint8 recipientAddressLength = 20;
        address recipientAddress = bridgerA;
        uint8 tokenID = BridgeUtils.ETH;
        uint64 amount = 100000000; // 1 ether in sui decimals

        bytes memory payload = abi.encodePacked(
            senderAddressLength,
            senderAddress,
            targetChain,
            recipientAddressLength,
            recipientAddress,
            tokenID,
            amount
        );

        BridgeUtils.Message memory message = BridgeUtils.Message({
            messageType: BridgeUtils.TOKEN_TRANSFER,
            version: 1,
            nonce: 1,
            chainID: 0,
            payload: payload
        });

        bytes memory encodedMessage = BridgeUtils.encodeMessage(message);
        bytes32 messageHash = keccak256(encodedMessage);

        bytes[] memory signatures = new bytes[](4);
        signatures[0] = getSignature(messageHash, committeeMemberPkA);
        signatures[1] = getSignature(messageHash, committeeMemberPkB);
        signatures[2] = getSignature(messageHash, committeeMemberPkC);
        signatures[3] = getSignature(messageHash, committeeMemberPkD);

        uint256 aBalance = bridgerA.balance;
        bridgeV2.transferBridgedTokensWithSignatures(signatures, message);
        assertEq(bridgerA.balance, aBalance + 1 ether);
    }

    /* ========== UPGRADE FROM V1 TO V2 TESTS ========== */

    function testUpgradeFromV1ToV2() public {
        // Deploy V1 first
        address _bridgeV1 = Upgrades.deployUUPSProxy(
            "SuiBridge.sol",
            abi.encodeCall(
                SuiBridge.initialize, (address(committee), address(vault), address(limiter))
            ),
            opts
        );

        SuiBridge bridgeV1 = SuiBridge(_bridgeV1);

        // Transfer ownership to V1
        changePrank(address(bridgeV2));
        vault.transferOwnership(address(bridgeV1));
        limiter.transferOwnership(address(bridgeV1));
        changePrank(deployer);

        // Deploy V2 implementation
        SuiBridgeV2 v2Implementation = new SuiBridgeV2();

        // Create upgrade message
        bytes memory payload = abi.encode(address(bridgeV1), address(v2Implementation), bytes(""));

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

        // Perform upgrade
        bridgeV1.upgradeWithSignatures(signatures, message);

        // Verify upgrade - implementation address should match V2
        assertEq(Upgrades.getImplementationAddress(address(bridgeV1)), address(v2Implementation));

        // Cast to V2 and verify V2 functions are accessible
        SuiBridgeV2 upgradedBridge = SuiBridgeV2(address(bridgeV1));

        // Verify V2 deposit works: fund the deployer and test bridgeETHV2
        changePrank(deployer);
        vm.deal(deployer, 2 ether);
        uint64 nonceBefore = upgradedBridge.nonces(BridgeUtils.TOKEN_TRANSFER);
        upgradedBridge.bridgeETHV2{value: 1 ether}(
            hex"06bb77410cd326430fa2036c8282dbb54a6f8640cea16ef5eff32d638718b3e4", 0
        );
        assertEq(upgradedBridge.nonces(BridgeUtils.TOKEN_TRANSFER), nonceBefore + 1);
    }

    /* ========== HELPER FUNCTIONS ========== */

    function _createV2TransferPayload(
        address recipient,
        uint8 tokenID,
        uint64 amount,
        uint256 timestamp
    ) internal pure returns (bytes memory) {
        // V2 payload format (72 bytes):
        // byte 0: sender address length (32)
        // bytes 1-32: sender address
        // byte 33: target chain id
        // byte 34: recipient address length (20)
        // bytes 35-54: recipient address
        // byte 55: token id
        // bytes 56-63: amount (8 bytes, big-endian)
        // bytes 64-71: timestamp (8 bytes, big-endian)

        bytes memory senderAddress = abi.encode(0); // 32 bytes
        uint8 targetChainID = 12; // _chainID

        return abi.encodePacked(
            uint8(32), // sender address length
            senderAddress,
            targetChainID,
            uint8(20), // recipient address length
            recipient,
            tokenID,
            amount,
            uint64(timestamp) // 8 bytes for timestamp
        );
    }

    function _pauseBridge() internal {
        // Create emergency op message to pause
        BridgeUtils.Message memory message = BridgeUtils.Message({
            messageType: BridgeUtils.EMERGENCY_OP,
            version: 1,
            nonce: 0,
            chainID: _chainID,
            payload: bytes(hex"00")
        });

        bytes memory encodedMessage = BridgeUtils.encodeMessage(message);
        bytes32 messageHash = keccak256(encodedMessage);

        bytes[] memory signatures = new bytes[](4);
        signatures[0] = getSignature(messageHash, committeeMemberPkA);
        signatures[1] = getSignature(messageHash, committeeMemberPkB);
        signatures[2] = getSignature(messageHash, committeeMemberPkC);
        signatures[3] = getSignature(messageHash, committeeMemberPkD);

        bridgeV2.executeEmergencyOpWithSignatures(signatures, message);
    }
}
