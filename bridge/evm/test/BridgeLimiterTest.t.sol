// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import "./BridgeBaseTest.t.sol";

contract BridgeLimiterTest is BridgeBaseTest {
    function setUp() public {
        setUpBridgeTest();
        // warp to next nearest hour start
        vm.warp(block.timestamp - (block.timestamp % 1 hours));
    }

    function testBridgeLimiterInitialization() public {
        assertEq(limiter.tokenPrices(0), SUI_PRICE);
        assertEq(limiter.tokenPrices(1), BTC_PRICE);
        assertEq(limiter.tokenPrices(2), ETH_PRICE);
        assertEq(limiter.tokenPrices(3), USDC_PRICE);
        assertEq(limiter.oldestHourTimestamp(), uint32(block.timestamp / 1 hours));
        assertEq(limiter.totalLimit(), totalLimit);
    }

    function testCalculateAmountInUSD() public {
        uint8 tokenID = 1; // wBTC
        uint256 wBTCAmount = 100000000; // wBTC has 8 decimals
        uint256 actual = limiter.calculateAmountInUSD(tokenID, wBTCAmount);
        assertEq(actual, BTC_PRICE);
        tokenID = 2;
        uint256 ethAmount = 1 ether;
        actual = limiter.calculateAmountInUSD(tokenID, ethAmount);
        assertEq(actual, ETH_PRICE);
        tokenID = 3;
        uint256 usdcAmount = 1000000; // USDC has 6 decimals
        actual = limiter.calculateAmountInUSD(tokenID, usdcAmount);
        assertEq(actual, USDC_PRICE);
    }

    function testCalculateWindowLimit() public {
        changePrank(address(bridge));
        uint8 tokenID = 3;
        uint256 amount = 1000000; // USDC has 6 decimals
        limiter.recordBridgeTransfers(tokenID, amount);
        skip(1 hours);
        limiter.recordBridgeTransfers(tokenID, 2 * amount);
        skip(1 hours);
        uint256 actual = limiter.calculateWindowAmount();
        assertEq(actual, 30000);
        skip(22 hours);
        actual = limiter.calculateWindowAmount();
        assertEq(actual, 20000);
        skip(59 minutes);
        actual = limiter.calculateWindowAmount();
        assertEq(actual, 20000);
        skip(1 minutes);
        actual = limiter.calculateWindowAmount();
        assertEq(actual, 0);
    }

    function testAmountWillExceedLimit() public {
        changePrank(address(bridge));
        uint8 tokenID = 3;
        uint256 amount = 999999 * 1000000; // USDC has 6 decimals
        assertFalse(limiter.willAmountExceedLimit(tokenID, amount));
        limiter.recordBridgeTransfers(tokenID, amount);
        assertTrue(limiter.willAmountExceedLimit(tokenID, 2000000));
        assertFalse(limiter.willAmountExceedLimit(tokenID, 1000000));
    }

    function testUpdateBridgeTransfer() public {
        changePrank(address(bridge));
        uint8 tokenID = 1;
        uint256 amount = 100000000; // wBTC has 8 decimals
        limiter.recordBridgeTransfers(tokenID, amount);
        tokenID = 2;
        amount = 1 ether;
        limiter.recordBridgeTransfers(tokenID, amount);
        tokenID = 3;
        amount = 1000000; // USDC has 6 decimals
        limiter.recordBridgeTransfers(tokenID, amount);
        assertEq(
            limiter.hourlyTransferAmount(uint32(block.timestamp / 1 hours)),
            BTC_PRICE + ETH_PRICE + USDC_PRICE
        );
    }

    function testrecordBridgeTransfersGarbageCollection() public {
        changePrank(address(bridge));
        uint8 tokenID = 1;
        uint256 amount = 100000000; // wBTC has 8 decimals
        uint32 hourToDelete = uint32(block.timestamp / 1 hours);
        limiter.recordBridgeTransfers(tokenID, amount);
        uint256 deleteAmount = limiter.hourlyTransferAmount(hourToDelete);
        assertEq(deleteAmount, BTC_PRICE);
        skip(25 hours);
        limiter.recordBridgeTransfers(tokenID, amount);
        deleteAmount = limiter.hourlyTransferAmount(hourToDelete);
        assertEq(deleteAmount, 0);
    }

    function testUpdateTokenPriceWithSignatures() public {
        changePrank(address(bridge));
        bytes memory payload = abi.encodePacked(uint8(1), uint64(100000000));
        // Create a sample BridgeMessage
        BridgeMessage.Message memory message = BridgeMessage.Message({
            messageType: BridgeMessage.UPDATE_TOKEN_PRICE,
            version: 1,
            nonce: 0,
            chainID: chainID,
            payload: payload
        });

        bytes memory messageBytes = BridgeMessage.encodeMessage(message);
        bytes32 messageHash = keccak256(messageBytes);

        bytes[] memory signatures = new bytes[](4);
        signatures[0] = getSignature(messageHash, committeeMemberPkA);
        signatures[1] = getSignature(messageHash, committeeMemberPkB);
        signatures[2] = getSignature(messageHash, committeeMemberPkC);
        signatures[3] = getSignature(messageHash, committeeMemberPkD);

        // Call the updateTokenPriceWithSignatures function
        limiter.updateTokenPriceWithSignatures(signatures, message);

        // Assert that the token price has been updated correctly
        assertEq(limiter.tokenPrices(1), 100000000);
    }

    function testUpdateLimitWithSignatures() public {
        changePrank(address(bridge));
        uint8 sourceChainID = 1;
        uint64 newLimit = 1000000000;
        bytes memory payload = abi.encodePacked(sourceChainID, newLimit);
        // Create a sample BridgeMessage
        BridgeMessage.Message memory message = BridgeMessage.Message({
            messageType: BridgeMessage.UPDATE_BRIDGE_LIMIT,
            version: 1,
            nonce: 0,
            chainID: chainID,
            payload: payload
        });

        bytes memory messageBytes = BridgeMessage.encodeMessage(message);
        bytes32 messageHash = keccak256(messageBytes);

        bytes[] memory signatures = new bytes[](4);
        signatures[0] = getSignature(messageHash, committeeMemberPkA);
        signatures[1] = getSignature(messageHash, committeeMemberPkB);
        signatures[2] = getSignature(messageHash, committeeMemberPkC);
        signatures[3] = getSignature(messageHash, committeeMemberPkD);

        assertEq(limiter.totalLimit(), totalLimit);

        // Call the updateLimitWithSignatures function
        limiter.updateLimitWithSignatures(signatures, message);

        assertEq(limiter.totalLimit(), 1000000000);
    }

    // An e2e update limit regression test covering message ser/de and signature verification
    function testUpdateLimitRegressionTest() public {
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
        committee.initialize(_committee, _stake, 1);
        vault = new BridgeVault(wETH);
        uint256[] memory tokenPrices = new uint256[](4);
        tokenPrices[0] = 10000; // SUI PRICE
        tokenPrices[1] = 10000; // BTC PRICE
        tokenPrices[2] = 10000; // ETH PRICE
        tokenPrices[3] = 10000; // USDC PRICE
        uint64 totalLimit = 1000000;

        skip(2 days);
        limiter = new BridgeLimiter();
        limiter.initialize(address(committee), address(tokens), tokenPrices, totalLimit);
        bridge = new SuiBridge();
        uint8[] memory _supportedDestinationChains = new uint8[](1);
        _supportedDestinationChains[0] = 0;
        bridge.initialize(
            address(committee),
            address(tokens),
            address(vault),
            address(limiter),
            wETH,
            _supportedDestinationChains
        );
        vault.transferOwnership(address(bridge));
        limiter.transferOwnership(address(bridge));

        // Fill vault with WETH
        changePrank(deployer);
        IWETH9(wETH).deposit{value: 10 ether}();
        IERC20(wETH).transfer(address(vault), 10 ether);

        bytes memory payload = hex"0c00000002540be400";

        // Create transfer message
        BridgeMessage.Message memory message = BridgeMessage.Message({
            messageType: BridgeMessage.UPDATE_BRIDGE_LIMIT,
            version: 1,
            nonce: 15,
            chainID: 3,
            payload: payload
        });
        bytes memory encodedMessage = BridgeMessage.encodeMessage(message);
        bytes memory expectedEncodedMessage =
            hex"5355495f4252494447455f4d4553534147450301000000000000000f030c00000002540be400";

        assertEq(encodedMessage, expectedEncodedMessage);

        bytes[] memory signatures = new bytes[](2);

        // TODO: generate signatures
        // signatures[0] =
        //     hex"e1cf11b380855ff1d4a451ebc2fd68477cf701b7d4ec88da3082709fe95201a5061b4b60cf13815a80ba9dfead23e220506aa74c4a863ba045d95715b4cc6b6e00";
        // signatures[1] =
        //     hex"8ba9ec92c2d5a44ecc123182f689b901a93921fd35f581354fea20b25a0ded6d055b96a64bdda77dd5a62b93d29abe93640aa3c1a136348093cd7a2418c6bfa301";

        // committee.verifySignatures(signatures, message);

        // limiter.updateLimitWithSignatures(signatures, message);
        // assertEq(limiter.totalLimit(), 1_000_000_0000);
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
        committee.initialize(_committee, _stake, 1);
        vault = new BridgeVault(wETH);
        uint256[] memory tokenPrices = new uint256[](4);
        tokenPrices[0] = 10000; // SUI PRICE
        tokenPrices[1] = 10000; // BTC PRICE
        tokenPrices[2] = 10000; // ETH PRICE
        tokenPrices[3] = 10000; // USDC PRICE
        uint64 totalLimit = 1000000;

        skip(2 days);
        limiter = new BridgeLimiter();
        limiter.initialize(address(committee), address(tokens), tokenPrices, totalLimit);
        bridge = new SuiBridge();
        uint8[] memory _supportedDestinationChains = new uint8[](1);
        _supportedDestinationChains[0] = 0;
        bridge.initialize(
            address(committee),
            address(tokens),
            address(vault),
            address(limiter),
            wETH,
            _supportedDestinationChains
        );
        vault.transferOwnership(address(bridge));
        limiter.transferOwnership(address(bridge));

        // Fill vault with WETH
        changePrank(deployer);
        IWETH9(wETH).deposit{value: 10 ether}();
        IERC20(wETH).transfer(address(vault), 10 ether);

        bytes memory payload = hex"01000000003b9aca00";

        // Create transfer message
        BridgeMessage.Message memory message = BridgeMessage.Message({
            messageType: BridgeMessage.UPDATE_TOKEN_PRICE,
            version: 1,
            nonce: 266,
            chainID: 3,
            payload: payload
        });
        bytes memory encodedMessage = BridgeMessage.encodeMessage(message);
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
        // assertEq(limiter.tokenPrices(BridgeMessage.BTC), 100_000_0000);
    }
}
