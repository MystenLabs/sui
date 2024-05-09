// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Test} from "forge-std/Test.sol";
import {strings} from "solidity-stringutils/src/strings.sol";

import {Utils, ContractInfo} from "openzeppelin-foundry-upgrades/internal/Utils.sol";

/**
 * @dev Tests the Utils internal library.
 */
contract UtilsTest is Test {
    function testGetContractInfo_from_file() public view {
        ContractInfo memory info = Utils.getContractInfo("Greeter.sol", "out");

        assertEq(info.shortName, "Greeter");
        assertEq(info.contractPath, "test/contracts/Greeter.sol");

        assertEq(info.license, "MIT");
        assertEq(info.sourceCodeHash, "0xf9875b1fd90da13f5f990d5ba7e66481f4b7e13e4a8f57fa9145fe90a1cb9324"); // source code hash of Greeter.sol
    }

    function testGetContractInfo_from_fileAndName() public view {
        ContractInfo memory info = Utils.getContractInfo("MyContractFile.sol:MyContractName", "out");

        assertEq(info.shortName, "MyContractName");
        assertEq(info.contractPath, "test/contracts/MyContractFile.sol");
    }

    function testGetContractInfo_from_artifact() public view {
        ContractInfo memory info = Utils.getContractInfo("out/MyContractFile.sol/MyContractName.json", "out");

        assertEq(info.shortName, "MyContractName");
        assertEq(info.contractPath, "test/contracts/MyContractFile.sol");
    }

    function testGetContractInfo_wrongNameFormat() public {
        Invoker c = new Invoker();
        try c.getContractInfo("Foo", "out") {
            fail();
        } catch Error(string memory reason) {
            assertEq(
                reason,
                "Contract name Foo must be in the format MyContract.sol:MyContract or MyContract.sol or out/MyContract.sol/MyContract.json"
            );
        }
    }

    function testGetContractInfo_outDirTrailingSlash() public view {
        ContractInfo memory info = Utils.getContractInfo("Greeter.sol", "out/");

        assertEq(info.shortName, "Greeter");
        assertEq(info.contractPath, "test/contracts/Greeter.sol");
    }

    function testGetContractInfo_invalidOutDir() public {
        Invoker c = new Invoker();
        try c.getContractInfo("Greeter.sol", "invalidoutdir") {
            fail();
        } catch {}
    }

    function testGetFullyQualifiedName_from_file() public view {
        string memory fqName = Utils.getFullyQualifiedName("Greeter.sol", "out");

        assertEq(fqName, "test/contracts/Greeter.sol:Greeter");
    }

    function testGetFullyQualifiedName_from_fileAndName() public view {
        string memory fqName = Utils.getFullyQualifiedName("MyContractFile.sol:MyContractName", "out");

        assertEq(fqName, "test/contracts/MyContractFile.sol:MyContractName");
    }

    function testGetFullyQualifiedName_from_artifact() public view {
        string memory fqName = Utils.getFullyQualifiedName("out/MyContractFile.sol/MyContractName.json", "out");

        assertEq(fqName, "test/contracts/MyContractFile.sol:MyContractName");
    }

    function testGetFullyQualifiedName_wrongNameFormat() public {
        Invoker i = new Invoker();
        try i.getFullyQualifiedName("Foo", "out") {
            fail();
        } catch Error(string memory reason) {
            assertEq(
                reason,
                "Contract name Foo must be in the format MyContract.sol:MyContract or MyContract.sol or out/MyContract.sol/MyContract.json"
            );
        }
    }

    function testGetFullyQualifiedName_invalidOutDir() public {
        Invoker i = new Invoker();
        try i.getFullyQualifiedName("Greeter.sol", "invalidoutdir") {
            fail();
        } catch {}
    }

    function testGetOutDir() public view {
        assertEq(Utils.getOutDir(), "out");
    }

    using strings for *;

    function testGetBuildInfoFile() public {
        ContractInfo memory contractInfo = Utils.getContractInfo("Greeter.sol", "out");
        string memory buildInfoFile = Utils.getBuildInfoFile(
            contractInfo.sourceCodeHash,
            contractInfo.shortName,
            "out"
        );

        assertTrue(buildInfoFile.toSlice().startsWith("out/build-info".toSlice()));
        assertTrue(buildInfoFile.toSlice().endsWith(".json".toSlice()));
    }

    function testToBashCommand() public pure {
        string[] memory inputs = new string[](3);
        inputs[0] = "foo";
        inputs[1] = "param";
        inputs[2] = "--option";

        string[] memory bashCommand = Utils.toBashCommand(inputs, "bash");

        assertEq(bashCommand.length, 3);
        assertEq(bashCommand[0], "bash");
        assertEq(bashCommand[1], "-c");
        assertEq(bashCommand[2], "foo param --option");
    }
}

contract Invoker {
    function getContractInfo(string memory contractName, string memory outDir) public view {
        Utils.getContractInfo(contractName, outDir);
    }

    function getFullyQualifiedName(string memory contractName, string memory outDir) public view {
        Utils.getFullyQualifiedName(contractName, outDir);
    }
}
