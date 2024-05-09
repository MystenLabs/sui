// SPDX-License-Identifier: MIT
pragma solidity >=0.7.0 <0.9.0;

import "../src/Test.sol";

contract StdChainsMock is Test {
    function exposed_getChain(string memory chainAlias) public returns (Chain memory) {
        return getChain(chainAlias);
    }

    function exposed_getChain(uint256 chainId) public returns (Chain memory) {
        return getChain(chainId);
    }

    function exposed_setChain(string memory chainAlias, ChainData memory chainData) public {
        setChain(chainAlias, chainData);
    }

    function exposed_setFallbackToDefaultRpcUrls(bool useDefault) public {
        setFallbackToDefaultRpcUrls(useDefault);
    }
}

contract StdChainsTest is Test {
    function test_ChainRpcInitialization() public {
        // RPCs specified in `foundry.toml` should be updated.
        assertEq(getChain(1).rpcUrl, "https://mainnet.infura.io/v3/b1d3925804e74152b316ca7da97060d3");
        assertEq(getChain("optimism_goerli").rpcUrl, "https://goerli.optimism.io/");
        assertEq(getChain("arbitrum_one_goerli").rpcUrl, "https://goerli-rollup.arbitrum.io/rpc/");

        // Environment variables should be the next fallback
        assertEq(getChain("arbitrum_nova").rpcUrl, "https://nova.arbitrum.io/rpc");
        vm.setEnv("ARBITRUM_NOVA_RPC_URL", "myoverride");
        assertEq(getChain("arbitrum_nova").rpcUrl, "myoverride");
        vm.setEnv("ARBITRUM_NOVA_RPC_URL", "https://nova.arbitrum.io/rpc");

        // Cannot override RPCs defined in `foundry.toml`
        vm.setEnv("MAINNET_RPC_URL", "myoverride2");
        assertEq(getChain("mainnet").rpcUrl, "https://mainnet.infura.io/v3/b1d3925804e74152b316ca7da97060d3");

        // Other RPCs should remain unchanged.
        assertEq(getChain(31337).rpcUrl, "http://127.0.0.1:8545");
        assertEq(getChain("sepolia").rpcUrl, "https://sepolia.infura.io/v3/b9794ad1ddf84dfb8c34d6bb5dca2001");
    }

    // Named with a leading underscore to clarify this is not intended to be run as a normal test,
    // and is intended to be used in the below `test_Rpcs` test.
    function _testRpc(string memory rpcAlias) internal {
        string memory rpcUrl = getChain(rpcAlias).rpcUrl;
        vm.createSelectFork(rpcUrl);
    }

    // Ensure we can connect to the default RPC URL for each chain.
    // Currently commented out since this is slow and public RPCs are flaky, often resulting in failing CI.
    // function test_Rpcs() public {
    //     _testRpc("mainnet");
    //     _testRpc("goerli");
    //     _testRpc("sepolia");
    //     _testRpc("optimism");
    //     _testRpc("optimism_goerli");
    //     _testRpc("arbitrum_one");
    //     _testRpc("arbitrum_one_goerli");
    //     _testRpc("arbitrum_nova");
    //     _testRpc("polygon");
    //     _testRpc("polygon_mumbai");
    //     _testRpc("avalanche");
    //     _testRpc("avalanche_fuji");
    //     _testRpc("bnb_smart_chain");
    //     _testRpc("bnb_smart_chain_testnet");
    //     _testRpc("gnosis_chain");
    //     _testRpc("moonbeam");
    //     _testRpc("moonriver");
    //     _testRpc("moonbase");
    //     _testRpc("base_goerli");
    //     _testRpc("base");
    //     _testRpc("fraxtal");
    //     _testRpc("fraxtal_testnet");
    // }

    function test_ChainNoDefault() public {
        // We deploy a mock to properly test the revert.
        StdChainsMock stdChainsMock = new StdChainsMock();

        vm.expectRevert("StdChains getChain(string): Chain with alias \"does_not_exist\" not found.");
        stdChainsMock.exposed_getChain("does_not_exist");
    }

    function test_SetChainFirstFails() public {
        // We deploy a mock to properly test the revert.
        StdChainsMock stdChainsMock = new StdChainsMock();

        vm.expectRevert("StdChains setChain(string,ChainData): Chain ID 31337 already used by \"anvil\".");
        stdChainsMock.exposed_setChain("anvil2", ChainData("Anvil", 31337, "URL"));
    }

    function test_ChainBubbleUp() public {
        // We deploy a mock to properly test the revert.
        StdChainsMock stdChainsMock = new StdChainsMock();

        stdChainsMock.exposed_setChain("needs_undefined_env_var", ChainData("", 123456789, ""));
        vm.expectRevert(
            "Failed to resolve env var `UNDEFINED_RPC_URL_PLACEHOLDER` in `${UNDEFINED_RPC_URL_PLACEHOLDER}`: environment variable not found"
        );
        stdChainsMock.exposed_getChain("needs_undefined_env_var");
    }

    function test_CannotSetChain_ChainIdExists() public {
        // We deploy a mock to properly test the revert.
        StdChainsMock stdChainsMock = new StdChainsMock();

        stdChainsMock.exposed_setChain("custom_chain", ChainData("Custom Chain", 123456789, "https://custom.chain/"));

        vm.expectRevert('StdChains setChain(string,ChainData): Chain ID 123456789 already used by "custom_chain".');

        stdChainsMock.exposed_setChain("another_custom_chain", ChainData("", 123456789, ""));
    }

    function test_SetChain() public {
        setChain("custom_chain", ChainData("Custom Chain", 123456789, "https://custom.chain/"));
        Chain memory customChain = getChain("custom_chain");
        assertEq(customChain.name, "Custom Chain");
        assertEq(customChain.chainId, 123456789);
        assertEq(customChain.chainAlias, "custom_chain");
        assertEq(customChain.rpcUrl, "https://custom.chain/");
        Chain memory chainById = getChain(123456789);
        assertEq(chainById.name, customChain.name);
        assertEq(chainById.chainId, customChain.chainId);
        assertEq(chainById.chainAlias, customChain.chainAlias);
        assertEq(chainById.rpcUrl, customChain.rpcUrl);
        customChain.name = "Another Custom Chain";
        customChain.chainId = 987654321;
        setChain("another_custom_chain", customChain);
        Chain memory anotherCustomChain = getChain("another_custom_chain");
        assertEq(anotherCustomChain.name, "Another Custom Chain");
        assertEq(anotherCustomChain.chainId, 987654321);
        assertEq(anotherCustomChain.chainAlias, "another_custom_chain");
        assertEq(anotherCustomChain.rpcUrl, "https://custom.chain/");
        // Verify the first chain data was not overwritten
        chainById = getChain(123456789);
        assertEq(chainById.name, "Custom Chain");
        assertEq(chainById.chainId, 123456789);
    }

    function test_SetNoEmptyAlias() public {
        // We deploy a mock to properly test the revert.
        StdChainsMock stdChainsMock = new StdChainsMock();

        vm.expectRevert("StdChains setChain(string,ChainData): Chain alias cannot be the empty string.");
        stdChainsMock.exposed_setChain("", ChainData("", 123456789, ""));
    }

    function test_SetNoChainId0() public {
        // We deploy a mock to properly test the revert.
        StdChainsMock stdChainsMock = new StdChainsMock();

        vm.expectRevert("StdChains setChain(string,ChainData): Chain ID cannot be 0.");
        stdChainsMock.exposed_setChain("alias", ChainData("", 0, ""));
    }

    function test_GetNoChainId0() public {
        // We deploy a mock to properly test the revert.
        StdChainsMock stdChainsMock = new StdChainsMock();

        vm.expectRevert("StdChains getChain(uint256): Chain ID cannot be 0.");
        stdChainsMock.exposed_getChain(0);
    }

    function test_GetNoEmptyAlias() public {
        // We deploy a mock to properly test the revert.
        StdChainsMock stdChainsMock = new StdChainsMock();

        vm.expectRevert("StdChains getChain(string): Chain alias cannot be the empty string.");
        stdChainsMock.exposed_getChain("");
    }

    function test_ChainIdNotFound() public {
        // We deploy a mock to properly test the revert.
        StdChainsMock stdChainsMock = new StdChainsMock();

        vm.expectRevert("StdChains getChain(string): Chain with alias \"no_such_alias\" not found.");
        stdChainsMock.exposed_getChain("no_such_alias");
    }

    function test_ChainAliasNotFound() public {
        // We deploy a mock to properly test the revert.
        StdChainsMock stdChainsMock = new StdChainsMock();

        vm.expectRevert("StdChains getChain(uint256): Chain with ID 321 not found.");

        stdChainsMock.exposed_getChain(321);
    }

    function test_SetChain_ExistingOne() public {
        // We deploy a mock to properly test the revert.
        StdChainsMock stdChainsMock = new StdChainsMock();

        setChain("custom_chain", ChainData("Custom Chain", 123456789, "https://custom.chain/"));
        assertEq(getChain(123456789).chainId, 123456789);

        setChain("custom_chain", ChainData("Modified Chain", 999999999, "https://modified.chain/"));
        vm.expectRevert("StdChains getChain(uint256): Chain with ID 123456789 not found.");
        stdChainsMock.exposed_getChain(123456789);

        Chain memory modifiedChain = getChain(999999999);
        assertEq(modifiedChain.name, "Modified Chain");
        assertEq(modifiedChain.chainId, 999999999);
        assertEq(modifiedChain.rpcUrl, "https://modified.chain/");
    }

    function test_DontUseDefaultRpcUrl() public {
        // We deploy a mock to properly test the revert.
        StdChainsMock stdChainsMock = new StdChainsMock();

        // Should error if default RPCs flag is set to false.
        stdChainsMock.exposed_setFallbackToDefaultRpcUrls(false);
        vm.expectRevert();
        stdChainsMock.exposed_getChain(31337);
        vm.expectRevert();
        stdChainsMock.exposed_getChain("sepolia");
    }
}
