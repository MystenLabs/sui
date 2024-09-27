// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "forge-std/Test.sol";
import {Upgrades} from "openzeppelin-foundry-upgrades/Upgrades.sol";
import "openzeppelin-foundry-upgrades/Options.sol";
import "../contracts/BridgeCommittee.sol";
import "../contracts/BridgeVault.sol";
import "../contracts/BridgeLimiter.sol";
import "../contracts/SuiBridge.sol";
import "../contracts/BridgeConfig.sol";

contract BridgeBaseTest is Test {
    address committeeMemberA;
    address committeeMemberB;
    address committeeMemberC;
    address committeeMemberD;
    address committeeMemberE;

    uint256 committeeMemberPkA;
    uint256 committeeMemberPkB;
    uint256 committeeMemberPkC;
    uint256 committeeMemberPkD;
    uint256 committeeMemberPkE;

    address bridgerA;
    address bridgerB;
    address bridgerC;

    address deployer;

    // token addresses on mainnet
    address wETH = 0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2;
    address USDC = 0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48;
    address USDT = 0xdAC17F958D2ee523a2206206994597C13D831ec7;
    address wBTC = 0x2260FAC5E5542a773Aa44fBCfeDf7C193bc2C599;

    address wBTCWhale = 0x6daB3bCbFb336b29d06B9C793AEF7eaA57888922;
    address USDCWhale = 0x51eDF02152EBfb338e03E30d65C15fBf06cc9ECC;
    address USDTWhale = 0xa7C0D36c4698981FAb42a7d8c783674c6Fe2592d;

    uint64 USD_VALUE_MULTIPLIER = 100000000; // 8 DP accuracy

    uint64 SUI_PRICE = 1_28000000;
    uint64 BTC_PRICE = 43251_89000000;
    uint64 ETH_PRICE = 2596_96000000;
    uint64 USDC_PRICE = 1_00000000;
    uint64[] tokenPrices;
    uint8[] suiDecimals;
    uint8[] tokenIds;
    address[] supportedTokens;
    uint8[] supportedChains;

    uint8 public chainID = 1;
    uint64 totalLimit = 1_000_000 * USD_VALUE_MULTIPLIER;
    uint16 minStakeRequired = 10000;

    BridgeCommittee public committee;
    SuiBridge public bridge;
    BridgeVault public vault;
    BridgeLimiter public limiter;
    BridgeConfig public config;

    Options opts;

    function setUpBridgeTest() public {
        vm.createSelectFork(
            string.concat("https://mainnet.infura.io/v3/", vm.envString("INFURA_API_KEY"))
        );
        (committeeMemberA, committeeMemberPkA) = makeAddrAndKey("a");
        (committeeMemberB, committeeMemberPkB) = makeAddrAndKey("b");
        (committeeMemberC, committeeMemberPkC) = makeAddrAndKey("c");
        (committeeMemberD, committeeMemberPkD) = makeAddrAndKey("d");
        (committeeMemberE, committeeMemberPkE) = makeAddrAndKey("e");
        bridgerA = makeAddr("bridgerA");
        bridgerB = makeAddr("bridgerB");
        bridgerC = makeAddr("bridgerC");
        vm.deal(committeeMemberA, 1 ether);
        vm.deal(committeeMemberB, 1 ether);
        vm.deal(committeeMemberC, 1 ether);
        vm.deal(committeeMemberD, 1 ether);
        vm.deal(committeeMemberE, 1 ether);
        vm.deal(bridgerA, 1 ether);
        vm.deal(bridgerB, 1 ether);
        deployer = address(1);
        vm.startPrank(deployer);

        // deploy committee =====================================================================
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

        opts.unsafeSkipAllChecks = true;

        address _committee = Upgrades.deployUUPSProxy(
            "BridgeCommittee.sol",
            abi.encodeCall(BridgeCommittee.initialize, (_committeeList, _stake, minStakeRequired)),
            opts
        );

        committee = BridgeCommittee(_committee);

        // deploy config =====================================================================
        supportedTokens = new address[](5);
        supportedTokens[0] = address(0);
        supportedTokens[1] = wBTC;
        supportedTokens[2] = wETH;
        supportedTokens[3] = USDC;
        supportedTokens[4] = USDT;
        supportedChains = new uint8[](1);
        supportedChains[0] = 0;
        tokenPrices = new uint64[](5);
        suiDecimals = new uint8[](5);
        tokenIds = new uint8[](5);
        suiDecimals[0] = 9;
        suiDecimals[1] = 8;
        suiDecimals[2] = 8;
        suiDecimals[3] = 6;
        suiDecimals[4] = 6;
        tokenPrices[0] = SUI_PRICE;
        tokenPrices[1] = BTC_PRICE;
        tokenPrices[2] = ETH_PRICE;
        tokenPrices[3] = USDC_PRICE;
        tokenPrices[4] = USDC_PRICE;
        tokenIds[0] = 0;
        tokenIds[1] = 1;
        tokenIds[2] = 2;
        tokenIds[3] = 3;
        tokenIds[4] = 4;

        address _config = Upgrades.deployUUPSProxy(
            "BridgeConfig.sol",
            abi.encodeCall(
                BridgeConfig.initialize,
                (address(committee), chainID, supportedTokens, tokenPrices, tokenIds, suiDecimals, supportedChains)
            ),
            opts
        );

        config = BridgeConfig(_config);

        // initialize config in the bridge committee
        committee.initializeConfig(address(config));

        // deploy vault =====================================================================

        vault = new BridgeVault(wETH);

        // deploy limiter =====================================================================

        uint64[] memory chainLimits = new uint64[](1);
        chainLimits[0] = totalLimit;

        address _limiter = Upgrades.deployUUPSProxy(
            "BridgeLimiter.sol",
            abi.encodeCall(
                BridgeLimiter.initialize, (address(committee), supportedChains, chainLimits)
            ),
            opts
        );

        limiter = BridgeLimiter(_limiter);

        // deploy bridge =====================================================================

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
    }

    function testSkip() public {}

    // Helper function to get the signature components from an address
    function getSignature(bytes32 digest, uint256 privateKey) public pure returns (bytes memory) {
        // r and s are the outputs of the ECDSA signature
        // r,s and v are packed into the signature. It should be 65 bytes: 32 + 32 + 1
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(privateKey, digest);

        // pack v, r, s into 65bytes signature
        return abi.encodePacked(r, s, v);
    }
}
