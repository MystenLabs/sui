// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "./mocks/MockTokens.sol";
import "./BridgeBaseTest.t.sol";

contract BridgeTokensTest is BridgeBaseTest {
    function setUp() public {
        setUpBridgeTest();
    }

    function testBridgeTokensInitialization() public {
        assertTrue(tokens.getAddress(1) == wBTC);
        assertTrue(tokens.getAddress(2) == wETH);
        assertTrue(tokens.getAddress(3) == USDC);
        assertTrue(tokens.getAddress(4) == USDT);
        assertEq(tokens.getSuiDecimal(0), 9);
        assertEq(tokens.getSuiDecimal(1), 8);
        assertEq(tokens.getSuiDecimal(2), 8);
        assertEq(tokens.getSuiDecimal(3), 6);
        assertEq(tokens.getSuiDecimal(4), 6);
    }

    function testGetAddress() public {
        assertEq(tokens.getAddress(1), wBTC);
    }

    function testconvertERC20ToSuiDecimalAmountTooLargeForUint64() public {
        vm.expectRevert(bytes("BridgeTokens: Amount too large for uint64"));
        tokens.convertERC20ToSuiDecimal(BridgeMessage.ETH, type(uint256).max);
    }

    function testconvertERC20ToSuiDecimalTokenIdNotSupported() public {
        vm.expectRevert(bytes("BridgeTokens: Unsupported token"));
        tokens.convertERC20ToSuiDecimal(type(uint8).max, 10 ether);
    }

    function testconvertERC20ToSuiDecimalInvalidSuiDecimal() public {
        vm.startPrank(address(bridge));
        address smallUSDC = address(new MockSmallUSDC());
        address[] memory _supportedTokens = new address[](4);
        _supportedTokens[0] = wBTC;
        _supportedTokens[1] = wETH;
        _supportedTokens[2] = smallUSDC;
        _supportedTokens[3] = USDT;
        BridgeTokens newBridgeTokens = new BridgeTokens(_supportedTokens);
        vm.expectRevert(bytes("BridgeTokens: Invalid Sui decimal"));
        newBridgeTokens.convertERC20ToSuiDecimal(3, 100);
    }

    function testconvertSuiToERC20DecimalInvalidSuiDecimal() public {
        vm.startPrank(address(bridge));
        address smallUSDC = address(new MockSmallUSDC());
        address[] memory _supportedTokens = new address[](4);
        _supportedTokens[0] = wBTC;
        _supportedTokens[1] = wETH;
        _supportedTokens[2] = smallUSDC;
        _supportedTokens[3] = USDT;
        BridgeTokens newBridgeTokens = new BridgeTokens(_supportedTokens);
        vm.expectRevert(bytes("BridgeTokens: Invalid Sui decimal"));
        newBridgeTokens.convertSuiToERC20Decimal(3, 100);
    }

    function testIsTokenSupported() public {
        assertTrue(tokens.isTokenSupported(1));
        assertTrue(!tokens.isTokenSupported(0));
    }

    function testGetSuiDecimal() public {
        assertEq(tokens.getSuiDecimal(1), 8);
    }

    function testconvertERC20ToSuiDecimal() public {
        // ETH
        assertEq(IERC20Metadata(wETH).decimals(), 18);
        uint256 ethAmount = 10 ether;
        uint64 suiAmount = tokens.convertERC20ToSuiDecimal(BridgeMessage.ETH, ethAmount);
        assertEq(suiAmount, 10_000_000_00); // 10 * 10 ^ 8

        // USDC
        assertEq(IERC20Metadata(USDC).decimals(), 6);
        ethAmount = 50_000_000; // 50 USDC
        suiAmount = tokens.convertERC20ToSuiDecimal(BridgeMessage.USDC, ethAmount);
        assertEq(suiAmount, ethAmount);

        // USDT
        assertEq(IERC20Metadata(USDT).decimals(), 6);
        ethAmount = 60_000_000; // 60 USDT
        suiAmount = tokens.convertERC20ToSuiDecimal(BridgeMessage.USDT, ethAmount);
        assertEq(suiAmount, ethAmount);

        // BTC
        assertEq(IERC20Metadata(wBTC).decimals(), 8);
        ethAmount = 2_00_000_000; // 2 BTC
        suiAmount = tokens.convertERC20ToSuiDecimal(BridgeMessage.BTC, ethAmount);
        assertEq(suiAmount, ethAmount);
    }

    function testconvertSuiToERC20Decimal() public {
        // ETH
        assertEq(IERC20Metadata(wETH).decimals(), 18);
        uint64 suiAmount = 11_000_000_00; // 11 eth
        uint256 ethAmount = tokens.convertSuiToERC20Decimal(BridgeMessage.ETH, suiAmount);
        assertEq(ethAmount, 11 ether);

        // USDC
        assertEq(IERC20Metadata(USDC).decimals(), 6);
        suiAmount = 50_000_000; // 50 USDC
        ethAmount = tokens.convertSuiToERC20Decimal(BridgeMessage.USDC, suiAmount);
        assertEq(suiAmount, ethAmount);

        // USDT
        assertEq(IERC20Metadata(USDT).decimals(), 6);
        suiAmount = 50_000_000; // 50 USDT
        ethAmount = tokens.convertSuiToERC20Decimal(BridgeMessage.USDT, suiAmount);
        assertEq(suiAmount, ethAmount);

        // BTC
        assertEq(IERC20Metadata(wBTC).decimals(), 8);
        suiAmount = 3_000_000_00; // 3 BTC
        ethAmount = tokens.convertSuiToERC20Decimal(BridgeMessage.BTC, suiAmount);
        assertEq(suiAmount, ethAmount);
    }
}
