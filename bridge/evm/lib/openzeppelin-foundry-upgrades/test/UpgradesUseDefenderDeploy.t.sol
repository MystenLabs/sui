// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Test} from "forge-std/Test.sol";
import {Vm} from "forge-std/Vm.sol";

import {Upgrades, Options} from "openzeppelin-foundry-upgrades/Upgrades.sol";

import {Greeter} from "./contracts/Greeter.sol";
import {GreeterProxiable} from "./contracts/GreeterProxiable.sol";
import {GreeterV2} from "./contracts/GreeterV2.sol";
import {GreeterV2Proxiable} from "./contracts/GreeterV2Proxiable.sol";

import {strings} from "solidity-stringutils/src/strings.sol";

/**
 * @dev Tests that the `defender.useDefenderDeploy` flag is recognized in the Upgrades library.
 * These do not perform any actual deployments, but just checks that the Defender CLI is invoked and catches its error message since we are using a dev network.
 */
contract UpgradesUseDefenderDeployTest is Test {
    address constant CHEATCODE_ADDRESS = 0x7109709ECfa91a80626fF3989D68f67F5b1DD12D;

    using strings for *;

    Deployer d;

    function setUp() public {
        d = new Deployer();
    }

    function _assertDefenderNotAvailable(strings.slice memory slice) private pure {
        assertTrue(
            slice.contains(
                "The current network with chainId 31337 is not supported by OpenZeppelin Defender".toSlice()
            ) || slice.contains("DEFENDER_KEY and DEFENDER_SECRET must be set in environment variables".toSlice())
        );
    }

    function testDeployUUPSProxy() public {
        Options memory opts;
        opts.defender.useDefenderDeploy = true;

        try d.deployUUPSProxy("GreeterProxiable.sol", abi.encodeCall(GreeterProxiable.initialize, ("hello")), opts) {
            fail();
        } catch Error(string memory reason) {
            strings.slice memory slice = reason.toSlice();
            assertTrue(slice.contains("Failed to deploy contract GreeterProxiable.sol".toSlice()));
            _assertDefenderNotAvailable(slice);
        }
    }

    function testDeployTransparentProxy() public {
        Options memory opts;
        opts.defender.useDefenderDeploy = true;

        try d.deployTransparentProxy("Greeter.sol", msg.sender, abi.encodeCall(Greeter.initialize, ("hello")), opts) {
            fail();
        } catch Error(string memory reason) {
            strings.slice memory slice = reason.toSlice();
            assertTrue(slice.contains("Failed to deploy contract Greeter.sol".toSlice()));
            _assertDefenderNotAvailable(slice);
        }
    }

    function testUpgradeProxy() public {
        address proxy = Upgrades.deployUUPSProxy("GreeterProxiable.sol", abi.encodeCall(Greeter.initialize, ("hello")));

        Options memory opts;
        opts.defender.useDefenderDeploy = true;

        try
            d.upgradeProxy(proxy, "GreeterV2Proxiable.sol", abi.encodeCall(GreeterV2Proxiable.resetGreeting, ()), opts)
        {
            fail();
        } catch Error(string memory reason) {
            strings.slice memory slice = reason.toSlice();
            assertTrue(slice.contains("Failed to deploy contract GreeterV2Proxiable.sol".toSlice()));
            _assertDefenderNotAvailable(slice);
        }
    }

    function testDeployBeacon() public {
        Options memory opts;
        opts.defender.useDefenderDeploy = true;

        try d.deployBeacon("Greeter.sol", msg.sender, opts) {
            fail();
        } catch Error(string memory reason) {
            strings.slice memory slice = reason.toSlice();
            assertTrue(slice.contains("Failed to deploy contract Greeter.sol".toSlice()));
            _assertDefenderNotAvailable(slice);
        }
    }

    function testDeployBeaconProxy() public {
        address beacon = Upgrades.deployBeacon("Greeter.sol", msg.sender);

        Options memory opts;
        opts.defender.useDefenderDeploy = true;

        try d.deployBeaconProxy(beacon, abi.encodeCall(Greeter.initialize, ("hello")), opts) {
            fail();
        } catch Error(string memory reason) {
            strings.slice memory slice = reason.toSlice();
            // Note the below is not the implementation contract, because this function only deploys the BeaconProxy contract
            assertTrue(slice.contains("Failed to deploy contract BeaconProxy.sol".toSlice()));
            _assertDefenderNotAvailable(slice);
        }
    }

    function testUpgradeBeacon() public {
        address beacon = Upgrades.deployBeacon("Greeter.sol", msg.sender);

        Options memory opts;
        opts.defender.useDefenderDeploy = true;

        try d.upgradeBeacon(beacon, "GreeterV2.sol", opts) {
            fail();
        } catch Error(string memory reason) {
            strings.slice memory slice = reason.toSlice();
            assertTrue(slice.contains("Failed to deploy contract GreeterV2.sol".toSlice()));
            _assertDefenderNotAvailable(slice);
        }
    }

    function testPrepareUpgrade() public {
        Options memory opts;
        opts.defender.useDefenderDeploy = true;

        try d.prepareUpgrade("GreeterV2.sol", opts) {
            fail();
        } catch Error(string memory reason) {
            strings.slice memory slice = reason.toSlice();
            assertTrue(slice.contains("Failed to deploy contract GreeterV2.sol".toSlice()));
            _assertDefenderNotAvailable(slice);
        }
    }

    function testValidateImplementation() public {
        Options memory opts;
        opts.defender.useDefenderDeploy = true;

        // The above flag should be ignored when calling this function
        Upgrades.validateImplementation("Greeter.sol", opts);
    }

    function testValidateUpgrade() public {
        Options memory opts;
        opts.defender.useDefenderDeploy = true;

        // The above flag should be ignored when calling this function
        Upgrades.validateUpgrade("GreeterV2.sol", opts);
    }
}

contract Deployer {
    function deployUUPSProxy(
        string memory contractName,
        bytes memory data,
        Options memory opts
    ) public returns (address) {
        return Upgrades.deployUUPSProxy(contractName, data, opts);
    }

    function deployTransparentProxy(
        string memory contractName,
        address initialOwner,
        bytes memory data,
        Options memory opts
    ) public returns (address) {
        return Upgrades.deployTransparentProxy(contractName, initialOwner, data, opts);
    }

    function upgradeProxy(address proxy, string memory contractName, bytes memory data, Options memory opts) public {
        Upgrades.upgradeProxy(proxy, contractName, data, opts);
    }

    function deployBeacon(
        string memory contractName,
        address initialOwner,
        Options memory opts
    ) public returns (address) {
        return Upgrades.deployBeacon(contractName, initialOwner, opts);
    }

    function deployBeaconProxy(address beacon, bytes memory data, Options memory opts) public returns (address) {
        return Upgrades.deployBeaconProxy(beacon, data, opts);
    }

    function upgradeBeacon(address beacon, string memory contractName, Options memory opts) public {
        Upgrades.upgradeBeacon(beacon, contractName, opts);
    }

    function prepareUpgrade(string memory contractName, Options memory opts) public {
        Upgrades.prepareUpgrade(contractName, opts);
    }
}
