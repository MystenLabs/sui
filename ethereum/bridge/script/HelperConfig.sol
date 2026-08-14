// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.7;

contract HelperConfig {
    NetworkConfig public activeNetworkConfig;

    struct NetworkConfig {
        address oracle;
        bytes32 jobId;
        uint256 chainlinkFee;
        address link;
        uint256 updateInterval;
        address priceFeed;
        uint64 subscriptionId;
        address vrfCoordinator;
        bytes32 keyHash;
    }

    mapping(uint256 => NetworkConfig) public chainIdToNetworkConfig;

    constructor() {
        chainIdToNetworkConfig[11155111] = getSepoliaEthConfig();
        chainIdToNetworkConfig[31337] = getAnvilEthConfig();

        activeNetworkConfig = chainIdToNetworkConfig[block.chainid];
    }

    function getSepoliaEthConfig()
        internal
        pure
        returns (NetworkConfig memory sepoliaNetworkConfig)
    {
        sepoliaNetworkConfig = NetworkConfig({
            oracle: 0x6090149792dAAeE9D1D568c9f9a6F6B46AA29eFD,
            jobId: "ca98366cc7314957b8c012c72f05aeeb",
            chainlinkFee: 1e17,
            link: 0x779877A7B0D9E8603169DdbD7836e478b4624789,
            updateInterval: 60, // every minute
            priceFeed: 0x694AA1769357215DE4FAC081bf1f309aDC325306, // ETH / USD
            subscriptionId: 0, // UPDATE ME!
            vrfCoordinator: 0x8103B0A8A00be2DDC778e6e7eaa21791Cd364625,
            keyHash: 0x474e34a077df58807dbe9c96d3c009b23b3c6d0cce433e59bbf5b34f823bc56c
        });
    }

    function getAnvilEthConfig()
        internal
        pure
        returns (NetworkConfig memory anvilNetworkConfig)
    {
        anvilNetworkConfig = NetworkConfig({
            oracle: address(0), // This is a mock
            jobId: "6b88e0402e5d415eb946e528b8e0c7ba",
            chainlinkFee: 1e17,
            link: address(0), // This is a mock
            updateInterval: 60, // every minute
            priceFeed: address(0), // This is a mock
            subscriptionId: 0,
            vrfCoordinator: address(0), // This is a mock
            keyHash: 0xd89b2bf150e3b9e13446986e571fb9cab24b13cea0a43ea20a6049a85cc807cc
        });
    }
}
