// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "forge-std/Script.sol";
// import "openzeppelin-foundry-upgrades/Upgrades.sol";
import {Upgrades} from "openzeppelin-foundry-upgrades/Upgrades.sol";
import "@openzeppelin/contracts/utils/Strings.sol";
import "../contracts/BridgeCommittee.sol";
import "../contracts/BridgeVault.sol";
import "../contracts/utils/BridgeConfig.sol";
import "../contracts/BridgeLimiter.sol";
import "../contracts/SuiBridge.sol";
import "../test/mocks/MockTokens.sol";

contract DeployBridge is Script {
    function run() external {
        uint256 deployerPrivateKey = vm.envUint("PRIVATE_KEY");
        vm.startBroadcast(deployerPrivateKey);

        string memory chainID = Strings.toString(block.chainid);
        string memory root = vm.projectRoot();
        string memory path = string.concat(root, "/deploy_configs/", chainID, ".json");
        string memory json = vm.readFile(path);
        bytes memory bytesJson = vm.parseJson(json);
        DeployConfig memory config = abi.decode(bytesJson, (DeployConfig));

        // TODO: validate config values before deploying

        // if deploying to local network, deploy mock tokens
        if (keccak256(abi.encode(chainID)) == keccak256(abi.encode("31337"))) {
            // deploy WETH
            config.WETH = address(new WETH());

            // deploy mock tokens
            MockWBTC wBTC = new MockWBTC();
            MockUSDC USDC = new MockUSDC();
            MockUSDT USDT = new MockUSDT();

            // update config with mock addresses
            config.supportedTokens = new address[](4);
            // In BridgeConfig.sol `supportedTokens is shifted by one
            // and the first token is SUI.
            config.supportedTokens[0] = address(wBTC);
            config.supportedTokens[1] = config.WETH;
            config.supportedTokens[2] = address(USDC);
            config.supportedTokens[3] = address(USDT);
        }

        // convert supported chains from uint256 to uint8[]
        uint8[] memory supportedChainIDs = new uint8[](config.supportedChainIDs.length);
        for (uint256 i; i < config.supportedChainIDs.length; i++) {
            supportedChainIDs[i] = uint8(config.supportedChainIDs[i]);
        }

        // deploy bridge config

        BridgeConfig bridgeConfig =
            new BridgeConfig(uint8(config.sourceChainId), config.supportedTokens, supportedChainIDs);

        // deploy Bridge Committee

        // convert committeeMembers stake from uint256 to uint16[]
        uint16[] memory committeeMemberStake = new uint16[](config.committeeMemberStake.length);
        for (uint256 i; i < config.committeeMemberStake.length; i++) {
            committeeMemberStake[i] = uint16(config.committeeMemberStake[i]);
        }

        address bridgeCommittee = Upgrades.deployUUPSProxy(
            "BridgeCommittee.sol",
            abi.encodeCall(
                BridgeCommittee.initialize,
                (
                    address(bridgeConfig),
                    config.committeeMembers,
                    committeeMemberStake,
                    uint16(config.minCommitteeStakeRequired)
                )
            )
        );

        // deploy vault

        BridgeVault vault = new BridgeVault(config.WETH);

        // deploy limiter

        // convert chain limits from uint256 to uint64[]
        uint64[] memory chainLimits = new uint64[](config.supportedChainLimitsInDollars.length);
        for (uint256 i; i < config.supportedChainLimitsInDollars.length; i++) {
            chainLimits[i] = uint64(config.supportedChainLimitsInDollars[i]);
        }

        address limiter = Upgrades.deployUUPSProxy(
            "BridgeLimiter.sol",
            abi.encodeCall(
                BridgeLimiter.initialize,
                (bridgeCommittee, config.tokenPrices, supportedChainIDs, chainLimits)
            )
        );

        uint8[] memory _destinationChains = new uint8[](1);
        _destinationChains[0] = 1;

        // deploy Sui Bridge

        address suiBridge = Upgrades.deployUUPSProxy(
            "SuiBridge.sol",
            abi.encodeCall(
                SuiBridge.initialize, (bridgeCommittee, address(vault), limiter, config.WETH)
            )
        );

        // transfer vault ownership to bridge
        vault.transferOwnership(suiBridge);
        // transfer limiter ownership to bridge
        BridgeLimiter instance = BridgeLimiter(limiter);
        instance.transferOwnership(suiBridge);

        // print deployed addresses for post deployment setup
        console.log("[Deployed] BridgeConfig:", address(bridgeConfig));
        console.log("[Deployed] SuiBridge:", suiBridge);
        console.log("[Deployed] BridgeLimiter:", limiter);
        console.log("[Deployed] BridgeCommittee:", bridgeCommittee);
        console.log("[Deployed] BridgeVault:", address(vault));
        console.log("[Deployed] BTC:", bridgeConfig.getTokenAddress(1));
        console.log("[Deployed] ETH:", bridgeConfig.getTokenAddress(2));
        console.log("[Deployed] USDC:", bridgeConfig.getTokenAddress(3));
        console.log("[Deployed] USDT:", bridgeConfig.getTokenAddress(4));

        vm.stopBroadcast();
    }

    // used to ignore for forge coverage
    function test() public {}
}

/// check the following for guidelines on updating deploy_configs and references:
/// https://book.getfoundry.sh/cheatcodes/parse-json
struct DeployConfig {
    uint256[] committeeMemberStake;
    address[] committeeMembers;
    uint256 minCommitteeStakeRequired;
    uint256 sourceChainId;
    uint256[] supportedChainIDs;
    uint256[] supportedChainLimitsInDollars;
    address[] supportedTokens;
    uint256[] tokenPrices;
    address WETH;
}
