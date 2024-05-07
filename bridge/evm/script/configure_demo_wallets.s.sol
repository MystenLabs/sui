// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "forge-std/Script.sol";
import "../test/mocks/MockTokens.sol";

contract ConfigureDemoWallets is Script {
    function run() external {
        uint256 deployerPrivateKey = vm.envUint("PRIVATE_KEY");
        vm.startBroadcast(deployerPrivateKey);

        // TODO: add wallet addresses
        address[10] memory wallets = [
            address(0),
            address(0),
            address(0),
            address(0),
            address(0),
            address(0),
            address(0),
            address(0),
            address(0),
            address(0)
        ];

        // TODO: add token addresses
        MockWBTC wBTC = MockWBTC(address(1));
        WETH wETH = WETH(address(1));
        MockUSDC USDC = MockUSDC(address(1));
        MockUSDT USDT = MockUSDT(address(1));

        for (uint256 i = 0; i < wallets.length; i++) {
            wBTC.mint(wallets[i], 100000000);
            wETH.deposit{value: 1000000000000000000}();
            wETH.transfer(wallets[i], 1000000000000000000);
            USDC.mint(wallets[i], 1000000);
            USDT.mint(wallets[i], 1000000);
        }

        vm.stopBroadcast();
    }

    // used to ignore for forge coverage
    function testSkip() public {}
}
