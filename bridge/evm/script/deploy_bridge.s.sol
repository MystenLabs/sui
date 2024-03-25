// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "forge-std/Script.sol";
// import "openzeppelin-foundry-upgrades/Upgrades.sol";
import {Upgrades} from "openzeppelin-foundry-upgrades/Upgrades.sol";
import "@openzeppelin/contracts/utils/Strings.sol";
import "../contracts/BridgeCommittee.sol";
import "../contracts/BridgeVault.sol";
import "../contracts/BridgeConfig.sol";
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
        DeployConfig memory deployConfig = abi.decode(bytesJson, (DeployConfig));

        // TODO: validate config values before deploying

        // if deploying to local network, deploy mock tokens
        if (keccak256(abi.encode(chainID)) == keccak256(abi.encode("31337"))) {
            // deploy WETH
            deployConfig.WETH = address(new WETH());

            // deploy mock tokens
            MockWBTC wBTC = new MockWBTC();
            MockUSDC USDC = new MockUSDC();
            MockUSDC USDT = new MockUSDC();

            // update config with mock addresses
            deployConfig.supportedTokens = new address[](5);
            deployConfig.supportedTokens[0] = address(0);
            deployConfig.supportedTokens[1] = address(wBTC);
            deployConfig.supportedTokens[2] = deployConfig.WETH;
            deployConfig.supportedTokens[3] = address(USDC);
            deployConfig.supportedTokens[4] = address(USDT);
        }

        // convert supported chains from uint256 to uint8[]
        uint8[] memory supportedChainIDs = new uint8[](deployConfig.supportedChainIDs.length);
        for (uint256 i; i < deployConfig.supportedChainIDs.length; i++) {
            supportedChainIDs[i] = uint8(deployConfig.supportedChainIDs[i]);
        }

        // deploy Bridge Committee ===================================================================

        // convert committeeMembers stake from uint256 to uint16[]
        uint16[] memory committeeMemberStake =
            new uint16[](deployConfig.committeeMemberStake.length);
        for (uint256 i; i < deployConfig.committeeMemberStake.length; i++) {
            committeeMemberStake[i] = uint16(deployConfig.committeeMemberStake[i]);
        }

        address bridgeCommittee = Upgrades.deployUUPSProxy(
            "BridgeCommittee.sol",
            abi.encodeCall(
                BridgeCommittee.initialize,
                (
                    deployConfig.committeeMembers,
                    committeeMemberStake,
                    uint16(deployConfig.minCommitteeStakeRequired)
                )
            )
        );

        // deploy bridge config =====================================================================

        address bridgeConfig = Upgrades.deployUUPSProxy(
            "BridgeConfig.sol",
            abi.encodeCall(
                BridgeConfig.initialize,
                (
                    address(bridgeCommittee),
                    uint8(deployConfig.sourceChainId),
                    deployConfig.supportedTokens,
                    deployConfig.tokenPrices,
                    supportedChainIDs
                )
            )
        );

        // initialize config in the bridge committee
        BridgeCommittee(bridgeCommittee).initializeConfig(address(bridgeConfig));

        // deploy vault =============================================================================

        BridgeVault vault = new BridgeVault(deployConfig.WETH);

        // deploy limiter ===========================================================================

        // convert chain limits from uint256 to uint64[]
        uint64[] memory chainLimits =
            new uint64[](deployConfig.supportedChainLimitsInDollars.length);
        for (uint256 i; i < deployConfig.supportedChainLimitsInDollars.length; i++) {
            chainLimits[i] = uint64(deployConfig.supportedChainLimitsInDollars[i]);
        }

        address limiter = Upgrades.deployUUPSProxy(
            "BridgeLimiter.sol",
            abi.encodeCall(
                BridgeLimiter.initialize, (bridgeCommittee, supportedChainIDs, chainLimits)
            )
        );

        uint8[] memory _destinationChains = new uint8[](1);
        _destinationChains[0] = 1;

        // deploy Sui Bridge ========================================================================

        address suiBridge = Upgrades.deployUUPSProxy(
            "SuiBridge.sol",
            abi.encodeCall(
                SuiBridge.initialize, (bridgeCommittee, address(vault), limiter, deployConfig.WETH)
            )
        );

        // transfer vault ownership to bridge
        vault.transferOwnership(suiBridge);
        // transfer limiter ownership to bridge
        BridgeLimiter instance = BridgeLimiter(limiter);
        instance.transferOwnership(suiBridge);
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
