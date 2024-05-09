// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Test} from "forge-std/Test.sol";
import {Vm} from "forge-std/Vm.sol";

import {Upgrades, Options} from "openzeppelin-foundry-upgrades/Upgrades.sol";

import {Proxy} from "@openzeppelin/contracts/proxy/Proxy.sol";
import {IBeacon} from "@openzeppelin/contracts/proxy/beacon/IBeacon.sol";

import {Greeter} from "./contracts/Greeter.sol";
import {GreeterProxiable} from "./contracts/GreeterProxiable.sol";
import {GreeterV2} from "./contracts/GreeterV2.sol";
import {GreeterV2Proxiable} from "./contracts/GreeterV2Proxiable.sol";
import {WithConstructor, NoInitializer} from "./contracts/WithConstructor.sol";

// Import additional contracts to include them for compilation
import {MyContractName} from "./contracts/MyContractFile.sol";
import "./contracts/Validations.sol";

/**
 * @dev Tests for the Upgrades library.
 */
contract UpgradesTest is Test {
    address constant CHEATCODE_ADDRESS = 0x7109709ECfa91a80626fF3989D68f67F5b1DD12D;

    function testUUPS() public {
        address proxy = Upgrades.deployUUPSProxy("GreeterProxiable.sol", abi.encodeCall(Greeter.initialize, ("hello")));
        Greeter instance = Greeter(proxy);
        address implAddressV1 = Upgrades.getImplementationAddress(proxy);

        assertEq(instance.greeting(), "hello");

        Upgrades.upgradeProxy(
            proxy,
            "GreeterV2Proxiable.sol",
            abi.encodeCall(GreeterV2Proxiable.resetGreeting, ()),
            msg.sender
        );
        address implAddressV2 = Upgrades.getImplementationAddress(proxy);

        assertEq(instance.greeting(), "resetted");
        assertFalse(implAddressV2 == implAddressV1);
    }

    function testTransparent() public {
        address proxy = Upgrades.deployTransparentProxy(
            "Greeter.sol",
            msg.sender,
            abi.encodeCall(Greeter.initialize, ("hello"))
        );
        Greeter instance = Greeter(proxy);
        address implAddressV1 = Upgrades.getImplementationAddress(proxy);
        address adminAddress = Upgrades.getAdminAddress(proxy);

        assertFalse(adminAddress == address(0));

        assertEq(instance.greeting(), "hello");

        Upgrades.upgradeProxy(proxy, "GreeterV2.sol", abi.encodeCall(GreeterV2.resetGreeting, ()), msg.sender);
        address implAddressV2 = Upgrades.getImplementationAddress(proxy);

        assertEq(Upgrades.getAdminAddress(proxy), adminAddress);

        assertEq(instance.greeting(), "resetted");
        assertFalse(implAddressV2 == implAddressV1);
    }

    function testBeacon() public {
        address beacon = Upgrades.deployBeacon("Greeter.sol", msg.sender);
        address implAddressV1 = IBeacon(beacon).implementation();

        address proxy = Upgrades.deployBeaconProxy(beacon, abi.encodeCall(Greeter.initialize, ("hello")));
        Greeter instance = Greeter(proxy);

        assertEq(Upgrades.getBeaconAddress(proxy), beacon);

        assertEq(instance.greeting(), "hello");

        Upgrades.upgradeBeacon(beacon, "GreeterV2.sol", msg.sender);
        address implAddressV2 = IBeacon(beacon).implementation();

        GreeterV2(address(instance)).resetGreeting();

        assertEq(instance.greeting(), "resetted");
        assertFalse(implAddressV2 == implAddressV1);
    }

    function testUpgradeProxyWithoutCaller() public {
        address proxy = Upgrades.deployUUPSProxy(
            "GreeterProxiable.sol",
            abi.encodeCall(GreeterProxiable.initialize, ("hello"))
        );

        Vm vm = Vm(CHEATCODE_ADDRESS);
        vm.startPrank(msg.sender);
        Upgrades.upgradeProxy(proxy, "GreeterV2Proxiable.sol", abi.encodeCall(GreeterV2Proxiable.resetGreeting, ()));
        vm.stopPrank();
    }

    function testUpgradeBeaconWithoutCaller() public {
        address beacon = Upgrades.deployBeacon("Greeter.sol", msg.sender);

        Vm vm = Vm(CHEATCODE_ADDRESS);
        vm.startPrank(msg.sender);
        Upgrades.upgradeBeacon(beacon, "GreeterV2.sol");
        vm.stopPrank();
    }

    function testValidateImplementation() public {
        Options memory opts;
        Validator v = new Validator();
        try v.validateImplementation("Validations.sol:Unsafe", opts) {
            fail();
        } catch {
            // TODO: check error message
        }
    }

    function testValidateLayout() public {
        Options memory opts;
        opts.referenceContract = "Validations.sol:LayoutV1";
        Validator v = new Validator();
        try v.validateUpgrade("Validations.sol:LayoutV2_Bad", opts) {
            fail();
        } catch {
            // TODO: check error message
        }
    }

    function testValidateLayoutUpgradesFrom() public {
        Options memory opts;
        Validator v = new Validator();
        try v.validateUpgrade("Validations.sol:LayoutV2_UpgradesFrom_Bad", opts) {
            fail();
        } catch {
            // TODO: check error message
        }
    }

    function testValidateNamespaced() public {
        Options memory opts;
        opts.referenceContract = "Validations.sol:NamespacedV1";
        Validator v = new Validator();
        try v.validateUpgrade("Validations.sol:NamespacedV2_Bad", opts) {
            fail();
        } catch {
            // TODO: check error message
        }
    }

    function testValidateNamespacedUpgradesFrom() public {
        Options memory opts;
        Validator v = new Validator();
        try v.validateUpgrade("Validations.sol:NamespacedV2_UpgradesFrom_Bad", opts) {
            fail();
        } catch {
            // TODO: check error message
        }
    }

    function testValidateNamespacedOk() public {
        Options memory opts;
        opts.referenceContract = "Validations.sol:NamespacedV1";
        Upgrades.validateUpgrade("Validations.sol:NamespacedV2_Ok", opts);
    }

    function testValidateNamespacedUpgradesFromOk() public {
        Options memory opts;
        Upgrades.validateUpgrade("Validations.sol:NamespacedV2_UpgradesFrom_Ok", opts);
    }

    function testValidateNamespacedNoReference() public {
        Options memory opts;
        Validator v = new Validator();
        // validate upgrade without reference contract - an error is expected from upgrades-core CLI
        try v.validateUpgrade("Validations.sol:NamespacedV2_Ok", opts) {
            fail();
        } catch {
            // TODO: check error message
        }
    }

    function testUnsafeSkipAllChecks() public {
        Options memory opts;
        opts.unsafeSkipAllChecks = true;
        Upgrades.validateImplementation("Validations.sol:Unsafe", opts);
    }

    function testUnsafeSkipStorageCheck() public {
        Options memory opts;
        opts.unsafeSkipStorageCheck = true;
        Upgrades.validateUpgrade("Validations.sol:NamespacedV2_UpgradesFrom_Bad", opts);
    }

    function testUnsafeAllow() public {
        Options memory opts;
        opts.unsafeAllow = "delegatecall,selfdestruct";
        Upgrades.validateImplementation("Validations.sol:Unsafe", opts);
    }

    function testUnsafeAllowRenames() public {
        Options memory opts;
        opts.unsafeAllowRenames = true;
        Upgrades.validateImplementation("Validations.sol:LayoutV2_Renamed", opts);
    }

    function testSkipStorageCheckNoReference() public {
        Options memory opts;
        opts.unsafeSkipStorageCheck = true;
        Upgrades.validateUpgrade("Validations.sol:NamespacedV2_Ok", opts);
    }

    function testWithConstructor() public {
        Options memory opts;
        opts.constructorData = abi.encode(123);
        address proxy = Upgrades.deployTransparentProxy(
            "WithConstructor.sol:WithConstructor",
            msg.sender,
            abi.encodeCall(WithConstructor.initialize, (456)),
            opts
        );
        assertEq(WithConstructor(proxy).a(), 123);
        assertEq(WithConstructor(proxy).b(), 456);
    }

    function testNoInitializer() public {
        Options memory opts;
        opts.constructorData = abi.encode(123);
        address proxy = Upgrades.deployTransparentProxy("WithConstructor.sol:NoInitializer", msg.sender, "", opts);
        assertEq(WithConstructor(proxy).a(), 123);
    }
}

contract Validator {
    function validateImplementation(string memory contractName, Options memory opts) public {
        Upgrades.validateImplementation(contractName, opts);
    }

    function validateUpgrade(string memory contractName, Options memory opts) public {
        Upgrades.validateUpgrade(contractName, opts);
    }
}
