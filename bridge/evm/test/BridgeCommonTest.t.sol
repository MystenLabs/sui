// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "./mocks/MockTokens.sol";
import "./BridgeBaseTest.t.sol";

contract BridgeCommonTest is BridgeBaseTest {
    function setUp() public {
        setUpBridgeTest();
    }

    function testBridgeCommonInitialization() public {
        assertTrue(common.getTokenAddress(1) == wBTC);
        assertTrue(common.getTokenAddress(2) == wETH);
        assertTrue(common.getTokenAddress(3) == USDC);
        assertTrue(common.getTokenAddress(4) == USDT);
        assertEq(common.getSuiDecimal(0), 9);
        assertEq(common.getSuiDecimal(1), 8);
        assertEq(common.getSuiDecimal(2), 8);
        assertEq(common.getSuiDecimal(3), 6);
        assertEq(common.getSuiDecimal(4), 6);
        assertEq(common.chainID(), chainID);
        assertTrue(common.supportedChains(0));
    }

    function testGetAddress() public {
        assertEq(common.getTokenAddress(1), wBTC);
    }

    function testconvertERC20ToSuiDecimalAmountTooLargeForUint64() public {
        vm.expectRevert(bytes("BridgeCommon: Amount too large for uint64"));
        common.convertERC20ToSuiDecimal(BridgeMessage.ETH, type(uint256).max);
    }

    function testconvertERC20ToSuiDecimalTokenIdNotSupported() public {
        vm.expectRevert(bytes("BridgeCommon: Unsupported token"));
        common.convertERC20ToSuiDecimal(type(uint8).max, 10 ether);
    }

    function testconvertERC20ToSuiDecimalInvalidSuiDecimal() public {
        vm.startPrank(address(bridge));
        address smallUSDC = address(new MockSmallUSDC());
        address[] memory _supportedTokens = new address[](4);
        _supportedTokens[0] = wBTC;
        _supportedTokens[1] = wETH;
        _supportedTokens[2] = smallUSDC;
        _supportedTokens[3] = USDT;
        uint8[] memory _supportedDestinationChains = new uint8[](1);
        _supportedDestinationChains[0] = 0;
        BridgeCommon newBridgeCommon =
            new BridgeCommon(chainID, _supportedTokens, _supportedDestinationChains);
        vm.expectRevert(bytes("BridgeCommon: Invalid Sui decimal"));
        newBridgeCommon.convertERC20ToSuiDecimal(3, 100);
    }

    function testconvertSuiToERC20DecimalInvalidSuiDecimal() public {
        vm.startPrank(address(bridge));
        address smallUSDC = address(new MockSmallUSDC());
        address[] memory _supportedTokens = new address[](4);
        _supportedTokens[0] = wBTC;
        _supportedTokens[1] = wETH;
        _supportedTokens[2] = smallUSDC;
        _supportedTokens[3] = USDT;
        uint8[] memory _supportedDestinationChains = new uint8[](1);
        _supportedDestinationChains[0] = 0;
        BridgeCommon newBridgeCommon =
            new BridgeCommon(chainID, _supportedTokens, _supportedDestinationChains);
        vm.expectRevert(bytes("BridgeCommon: Invalid Sui decimal"));
        newBridgeCommon.convertSuiToERC20Decimal(3, 100);
    }

    function testIsTokenSupported() public {
        assertTrue(common.isTokenSupported(1));
        assertTrue(!common.isTokenSupported(0));
    }

    function testGetSuiDecimal() public {
        assertEq(common.getSuiDecimal(1), 8);
    }

    function testconvertERC20ToSuiDecimal() public {
        // ETH
        assertEq(IERC20Metadata(wETH).decimals(), 18);
        uint256 ethAmount = 10 ether;
        uint64 suiAmount = common.convertERC20ToSuiDecimal(BridgeMessage.ETH, ethAmount);
        assertEq(suiAmount, 10_000_000_00); // 10 * 10 ^ 8

        // USDC
        assertEq(IERC20Metadata(USDC).decimals(), 6);
        ethAmount = 50_000_000; // 50 USDC
        suiAmount = common.convertERC20ToSuiDecimal(BridgeMessage.USDC, ethAmount);
        assertEq(suiAmount, ethAmount);

        // USDT
        assertEq(IERC20Metadata(USDT).decimals(), 6);
        ethAmount = 60_000_000; // 60 USDT
        suiAmount = common.convertERC20ToSuiDecimal(BridgeMessage.USDT, ethAmount);
        assertEq(suiAmount, ethAmount);

        // BTC
        assertEq(IERC20Metadata(wBTC).decimals(), 8);
        ethAmount = 2_00_000_000; // 2 BTC
        suiAmount = common.convertERC20ToSuiDecimal(BridgeMessage.BTC, ethAmount);
        assertEq(suiAmount, ethAmount);
    }

    function testconvertSuiToERC20Decimal() public {
        // ETH
        assertEq(IERC20Metadata(wETH).decimals(), 18);
        uint64 suiAmount = 11_000_000_00; // 11 eth
        uint256 ethAmount = common.convertSuiToERC20Decimal(BridgeMessage.ETH, suiAmount);
        assertEq(ethAmount, 11 ether);

        // USDC
        assertEq(IERC20Metadata(USDC).decimals(), 6);
        suiAmount = 50_000_000; // 50 USDC
        ethAmount = common.convertSuiToERC20Decimal(BridgeMessage.USDC, suiAmount);
        assertEq(suiAmount, ethAmount);

        // USDT
        assertEq(IERC20Metadata(USDT).decimals(), 6);
        suiAmount = 50_000_000; // 50 USDT
        ethAmount = common.convertSuiToERC20Decimal(BridgeMessage.USDT, suiAmount);
        assertEq(suiAmount, ethAmount);

        // BTC
        assertEq(IERC20Metadata(wBTC).decimals(), 8);
        suiAmount = 3_000_000_00; // 3 BTC
        ethAmount = common.convertSuiToERC20Decimal(BridgeMessage.BTC, suiAmount);
        assertEq(suiAmount, ethAmount);
    }
}
