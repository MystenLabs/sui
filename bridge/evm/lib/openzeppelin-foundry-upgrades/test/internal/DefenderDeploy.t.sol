// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Test} from "forge-std/Test.sol";

import {Utils, ContractInfo} from "openzeppelin-foundry-upgrades/internal/Utils.sol";
import {DefenderDeploy} from "openzeppelin-foundry-upgrades/internal/DefenderDeploy.sol";
import {Versions} from "openzeppelin-foundry-upgrades/internal/Versions.sol";
import {Options, DefenderOptions} from "openzeppelin-foundry-upgrades/Options.sol";
import {ProposeUpgradeResponse, ApprovalProcessResponse} from "openzeppelin-foundry-upgrades/Defender.sol";
import {WithConstructor} from "../contracts/WithConstructor.sol";

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
                " --licenseType MIT"
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
                " --licenseType MIT --constructorBytecode 0x000000000000000000000000000000000000000000000000000000000000007b"
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
        opts.skipVerifySourceCode = true;
        opts.relayerId = "my-relayer-id";
        opts.salt = 0xabc0000000000000000000000000000000000000000000000000000000000123;

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
                " --licenseType MIT --constructorBytecode 0x000000000000000000000000000000000000000000000000000000000000007b --verifySourceCode false --relayerId my-relayer-id --salt 0xabc0000000000000000000000000000000000000000000000000000000000123"
            )
        );
    }

    function testBuildProposeUpgradeCommand() public {
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

    function testParseProposeUpgradeResponse() public {
        string memory output = "Upgrade proposal created.\nProposal ID: 123\nProposal URL: https://my.url/my-tx";

        ProposeUpgradeResponse memory response = DefenderDeploy.parseProposeUpgradeResponse(output);

        assertEq(response.proposalId, "123");
        assertEq(response.url, "https://my.url/my-tx");
    }

    function testParseProposeUpgradeResponseNoUrl() public {
        string memory output = "Upgrade proposal created.\nProposal ID: 123";

        ProposeUpgradeResponse memory response = DefenderDeploy.parseProposeUpgradeResponse(output);

        assertEq(response.proposalId, "123");
        assertEq(response.url, "");
    }

    function testBuildGetApprovalProcessCommand() public {
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

    function testParseApprovalProcessResponse() public {
        string
            memory output = "Approval process ID: abc\nVia: 0x1230000000000000000000000000000000000456\nVia type: Relayer";

        ApprovalProcessResponse memory response = DefenderDeploy.parseApprovalProcessResponse(output);

        assertEq(response.approvalProcessId, "abc");
        assertEq(response.via, 0x1230000000000000000000000000000000000456);
        assertEq(response.viaType, "Relayer");
    }

    function testParseApprovalProcessResponseIdOnly() public {
        string memory output = "Approval process ID: abc";

        ApprovalProcessResponse memory response = DefenderDeploy.parseApprovalProcessResponse(output);

        assertEq(response.approvalProcessId, "abc");
        assertTrue(response.via == address(0));
        assertEq(response.viaType, "");
    }
}
