// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Test} from "forge-std/Test.sol";

import {Utils, ContractInfo} from "openzeppelin-foundry-upgrades/internal/Utils.sol";
import {DefenderDeploy} from "openzeppelin-foundry-upgrades/internal/DefenderDeploy.sol";
import {Versions} from "openzeppelin-foundry-upgrades/internal/Versions.sol";
import {Options, DefenderOptions} from "openzeppelin-foundry-upgrades/Options.sol";
import {ProposeUpgradeResponse, ApprovalProcessResponse} from "openzeppelin-foundry-upgrades/Defender.sol";
import {WithConstructor} from "../contracts/WithConstructor.sol";
import {UnrecognizedLicense} from "../contracts/UnrecognizedLicense.sol";
import {NoLicense} from "../contracts/NoLicense.sol";
import {Unlicensed} from "../contracts/Unlicensed.sol";

/**
 * @dev Tests the DefenderDeploy internal library.
 */
contract DefenderDeployTest is Test {
    function _toString(string[] memory arr) private pure returns (string memory) {
        string memory result;
        for (uint i = 0; i < arr.length; i++) {
            result = string.concat(result, arr[i]);
            if (i < arr.length - 1) {
                result = string.concat(result, " ");
            }
        }
        return result;
    }

    function testBuildDeployCommand() public {
        ContractInfo memory contractInfo = Utils.getContractInfo("MyContractFile.sol:MyContractName", "out");
        string memory buildInfoFile = Utils.getBuildInfoFile(
            contractInfo.sourceCodeHash,
            contractInfo.shortName,
            "out"
        );

        DefenderOptions memory opts;
        string memory commandString = _toString(
            DefenderDeploy.buildDeployCommand(contractInfo, buildInfoFile, "", opts)
        );

        assertEq(
            commandString,
            string.concat(
                "npx @openzeppelin/defender-deploy-client-cli@",
                Versions.DEFENDER_DEPLOY_CLIENT_CLI,
                " deploy --contractName MyContractName --contractPath test/contracts/MyContractFile.sol --chainId 31337 --buildInfoFile ",
                buildInfoFile,
                ' --licenseType "MIT"'
            )
        );
    }

    function testBuildDeployCommandWithConstructorData() public {
        ContractInfo memory contractInfo = Utils.getContractInfo("WithConstructor.sol:WithConstructor", "out");
        string memory buildInfoFile = Utils.getBuildInfoFile(
            contractInfo.sourceCodeHash,
            contractInfo.shortName,
            "out"
        );

        bytes memory constructorData = abi.encode(123);

        DefenderOptions memory opts;
        string memory commandString = _toString(
            DefenderDeploy.buildDeployCommand(contractInfo, buildInfoFile, constructorData, opts)
        );

        assertEq(
            commandString,
            string.concat(
                "npx @openzeppelin/defender-deploy-client-cli@",
                Versions.DEFENDER_DEPLOY_CLIENT_CLI,
                " deploy --contractName WithConstructor --contractPath test/contracts/WithConstructor.sol --chainId 31337 --buildInfoFile ",
                buildInfoFile,
                ' --constructorBytecode 0x000000000000000000000000000000000000000000000000000000000000007b --licenseType "MIT"'
            )
        );
    }

    function testBuildDeployCommandAllCliOptions() public {
        ContractInfo memory contractInfo = Utils.getContractInfo("WithConstructor.sol:WithConstructor", "out");
        string memory buildInfoFile = Utils.getBuildInfoFile(
            contractInfo.sourceCodeHash,
            contractInfo.shortName,
            "out"
        );

        bytes memory constructorData = abi.encode(123);

        DefenderOptions memory opts;
        opts.useDefenderDeploy = true;
        opts.relayerId = "my-relayer-id";
        opts.salt = 0xabc0000000000000000000000000000000000000000000000000000000000123;
        opts.licenseType = "My License Type"; // not a valid type, but this just sets the option
        opts.txOverrides.gasLimit = 100000;
        opts.txOverrides.gasPrice = 1 gwei;
        opts.txOverrides.maxFeePerGas = 2 gwei;
        opts.txOverrides.maxPriorityFeePerGas = 0.5 gwei;

        string memory commandString = _toString(
            DefenderDeploy.buildDeployCommand(contractInfo, buildInfoFile, constructorData, opts)
        );

        assertEq(
            commandString,
            string.concat(
                "npx @openzeppelin/defender-deploy-client-cli@",
                Versions.DEFENDER_DEPLOY_CLIENT_CLI,
                " deploy --contractName WithConstructor --contractPath test/contracts/WithConstructor.sol --chainId 31337 --buildInfoFile ",
                buildInfoFile,
                ' --constructorBytecode 0x000000000000000000000000000000000000000000000000000000000000007b --licenseType "My License Type" --relayerId my-relayer-id --salt 0xabc0000000000000000000000000000000000000000000000000000000000123 --gasLimit 100000 --gasPrice 1000000000 --maxFeePerGas 2000000000 --maxPriorityFeePerGas 500000000'
            )
        );
    }

    function testBuildDeployCommandSkipVerifySourceCode() public {
        ContractInfo memory contractInfo = Utils.getContractInfo("WithConstructor.sol:WithConstructor", "out");
        string memory buildInfoFile = Utils.getBuildInfoFile(
            contractInfo.sourceCodeHash,
            contractInfo.shortName,
            "out"
        );

        bytes memory constructorData = abi.encode(123);

        DefenderOptions memory opts;
        opts.skipVerifySourceCode = true;

        string memory commandString = _toString(
            DefenderDeploy.buildDeployCommand(contractInfo, buildInfoFile, constructorData, opts)
        );

        assertEq(
            commandString,
            string.concat(
                "npx @openzeppelin/defender-deploy-client-cli@",
                Versions.DEFENDER_DEPLOY_CLIENT_CLI,
                " deploy --contractName WithConstructor --contractPath test/contracts/WithConstructor.sol --chainId 31337 --buildInfoFile ",
                buildInfoFile,
                " --constructorBytecode 0x000000000000000000000000000000000000000000000000000000000000007b --verifySourceCode false"
            )
        );
    }

    function testBuildDeployCommandSkipLicenseType() public {
        ContractInfo memory contractInfo = Utils.getContractInfo("WithConstructor.sol:WithConstructor", "out");
        string memory buildInfoFile = Utils.getBuildInfoFile(
            contractInfo.sourceCodeHash,
            contractInfo.shortName,
            "out"
        );

        bytes memory constructorData = abi.encode(123);

        DefenderOptions memory opts;
        opts.skipLicenseType = true;

        string memory commandString = _toString(
            DefenderDeploy.buildDeployCommand(contractInfo, buildInfoFile, constructorData, opts)
        );

        assertEq(
            commandString,
            string.concat(
                "npx @openzeppelin/defender-deploy-client-cli@",
                Versions.DEFENDER_DEPLOY_CLIENT_CLI,
                " deploy --contractName WithConstructor --contractPath test/contracts/WithConstructor.sol --chainId 31337 --buildInfoFile ",
                buildInfoFile,
                " --constructorBytecode 0x000000000000000000000000000000000000000000000000000000000000007b"
            )
        );
    }

    function testBuildDeployCommand_error_licenseType_skipLicenseType() public {
        ContractInfo memory contractInfo = Utils.getContractInfo("WithConstructor.sol:WithConstructor", "out");
        string memory buildInfoFile = Utils.getBuildInfoFile(
            contractInfo.sourceCodeHash,
            contractInfo.shortName,
            "out"
        );

        bytes memory constructorData = abi.encode(123);

        DefenderOptions memory opts;
        opts.skipLicenseType = true;
        opts.licenseType = "MyLicenseType";

        Invoker i = new Invoker();
        try i.buildDeployCommand(contractInfo, buildInfoFile, constructorData, opts) {
            fail();
        } catch Error(string memory reason) {
            assertEq(reason, "The `licenseType` option cannot be used when the `skipLicenseType` option is `true`");
        }
    }

    function testBuildDeployCommand_error_licenseType_skipVerifySourceCode() public {
        ContractInfo memory contractInfo = Utils.getContractInfo("WithConstructor.sol:WithConstructor", "out");
        string memory buildInfoFile = Utils.getBuildInfoFile(
            contractInfo.sourceCodeHash,
            contractInfo.shortName,
            "out"
        );

        bytes memory constructorData = abi.encode(123);

        DefenderOptions memory opts;
        opts.skipVerifySourceCode = true;
        opts.licenseType = "MyLicenseType";

        Invoker i = new Invoker();
        try i.buildDeployCommand(contractInfo, buildInfoFile, constructorData, opts) {
            fail();
        } catch Error(string memory reason) {
            assertEq(
                reason,
                "The `licenseType` option cannot be used when the `skipVerifySourceCode` option is `true`"
            );
        }
    }

    function testBuildDeployCommand_error_unrecognizedLicense() public {
        ContractInfo memory contractInfo = Utils.getContractInfo("UnrecognizedLicense.sol:UnrecognizedLicense", "out");
        string memory buildInfoFile = Utils.getBuildInfoFile(
            contractInfo.sourceCodeHash,
            contractInfo.shortName,
            "out"
        );

        DefenderOptions memory opts;

        Invoker i = new Invoker();
        try i.buildDeployCommand(contractInfo, buildInfoFile, "", opts) {
            fail();
        } catch Error(string memory reason) {
            assertEq(
                reason,
                "SPDX license identifier UnrecognizedId in test/contracts/UnrecognizedLicense.sol does not look like a supported license for block explorer verification. Use the `licenseType` option to specify a license type, or set the `skipLicenseType` option to `true` to skip."
            );
        }
    }

    function testBuildDeployCommandNoContractLicense() public {
        ContractInfo memory contractInfo = Utils.getContractInfo("NoLicense.sol:NoLicense", "out");
        string memory buildInfoFile = Utils.getBuildInfoFile(
            contractInfo.sourceCodeHash,
            contractInfo.shortName,
            "out"
        );

        DefenderOptions memory opts;
        string memory commandString = _toString(
            DefenderDeploy.buildDeployCommand(contractInfo, buildInfoFile, "", opts)
        );

        assertEq(
            commandString,
            string.concat(
                "npx @openzeppelin/defender-deploy-client-cli@",
                Versions.DEFENDER_DEPLOY_CLIENT_CLI,
                " deploy --contractName NoLicense --contractPath test/contracts/NoLicense.sol --chainId 31337 --buildInfoFile ",
                buildInfoFile
            )
        );
    }

    function testBuildDeployCommandUnlicensed() public {
        ContractInfo memory contractInfo = Utils.getContractInfo("Unlicensed.sol:Unlicensed", "out");
        string memory buildInfoFile = Utils.getBuildInfoFile(
            contractInfo.sourceCodeHash,
            contractInfo.shortName,
            "out"
        );

        DefenderOptions memory opts;
        string memory commandString = _toString(
            DefenderDeploy.buildDeployCommand(contractInfo, buildInfoFile, "", opts)
        );

        assertEq(
            commandString,
            string.concat(
                "npx @openzeppelin/defender-deploy-client-cli@",
                Versions.DEFENDER_DEPLOY_CLIENT_CLI,
                " deploy --contractName Unlicensed --contractPath test/contracts/Unlicensed.sol --chainId 31337 --buildInfoFile ",
                buildInfoFile,
                ' --licenseType "None"'
            )
        );
    }

    function testBuildProposeUpgradeCommand() public view {
        ContractInfo memory contractInfo = Utils.getContractInfo("MyContractFile.sol:MyContractName", "out");

        Options memory opts;
        string memory commandString = _toString(
            DefenderDeploy.buildProposeUpgradeCommand(
                address(0x1230000000000000000000000000000000000456),
                address(0),
                address(0x1110000000000000000000000000000000000222),
                contractInfo,
                opts
            )
        );

        assertEq(
            commandString,
            string.concat(
                "npx @openzeppelin/defender-deploy-client-cli@",
                Versions.DEFENDER_DEPLOY_CLIENT_CLI,
                " proposeUpgrade --proxyAddress 0x1230000000000000000000000000000000000456 --newImplementationAddress 0x1110000000000000000000000000000000000222 --chainId 31337 --contractArtifactFile ",
                contractInfo.artifactPath
            )
        );
    }

    function testParseProposeUpgradeResponse() public pure {
        string memory output = "Upgrade proposal created.\nProposal ID: 123\nProposal URL: https://my.url/my-tx";

        ProposeUpgradeResponse memory response = DefenderDeploy.parseProposeUpgradeResponse(output);

        assertEq(response.proposalId, "123");
        assertEq(response.url, "https://my.url/my-tx");
    }

    function testParseProposeUpgradeResponseNoUrl() public pure {
        string memory output = "Upgrade proposal created.\nProposal ID: 123";

        ProposeUpgradeResponse memory response = DefenderDeploy.parseProposeUpgradeResponse(output);

        assertEq(response.proposalId, "123");
        assertEq(response.url, "");
    }

    function testBuildGetApprovalProcessCommand() public view {
        string memory commandString = _toString(
            DefenderDeploy.buildGetApprovalProcessCommand("getDeployApprovalProcess")
        );

        assertEq(
            commandString,
            string.concat(
                "npx @openzeppelin/defender-deploy-client-cli@",
                Versions.DEFENDER_DEPLOY_CLIENT_CLI,
                " getDeployApprovalProcess --chainId 31337"
            )
        );
    }

    function testParseApprovalProcessResponse() public pure {
        string
            memory output = "Approval process ID: abc\nVia: 0x1230000000000000000000000000000000000456\nVia type: Relayer";

        ApprovalProcessResponse memory response = DefenderDeploy.parseApprovalProcessResponse(output);

        assertEq(response.approvalProcessId, "abc");
        assertEq(response.via, 0x1230000000000000000000000000000000000456);
        assertEq(response.viaType, "Relayer");
    }

    function testParseApprovalProcessResponseIdOnly() public pure {
        string memory output = "Approval process ID: abc";

        ApprovalProcessResponse memory response = DefenderDeploy.parseApprovalProcessResponse(output);

        assertEq(response.approvalProcessId, "abc");
        assertTrue(response.via == address(0));
        assertEq(response.viaType, "");
    }
}

contract Invoker {
    function buildDeployCommand(
        ContractInfo memory contractInfo,
        string memory buildInfoFile,
        bytes memory constructorData,
        DefenderOptions memory defenderOpts
    ) public view {
        DefenderDeploy.buildDeployCommand(contractInfo, buildInfoFile, constructorData, defenderOpts);
    }
}
