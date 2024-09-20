// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Vm} from "forge-std/Vm.sol";
import {console} from "forge-std/console.sol";
import {strings} from "solidity-stringutils/src/strings.sol";

import {Strings} from "@openzeppelin/contracts/utils/Strings.sol";

import {Utils, ContractInfo} from "./Utils.sol";
import {Versions} from "./Versions.sol";
import {Options, DefenderOptions} from "../Options.sol";
import {ProposeUpgradeResponse, ApprovalProcessResponse} from "../Defender.sol";

/**
 * @dev Internal helper methods for Defender deployments.
 *
 * DO NOT USE DIRECTLY. Use Defender.sol instead.
 */
library DefenderDeploy {
    using strings for *;

    function deploy(
        string memory contractName,
        bytes memory constructorData,
        DefenderOptions memory defenderOpts
    ) internal returns (address) {
        string memory outDir = Utils.getOutDir();
        ContractInfo memory contractInfo = Utils.getContractInfo(contractName, outDir);
        string memory buildInfoFile = Utils.getBuildInfoFile(
            contractInfo.sourceCodeHash,
            contractInfo.shortName,
            outDir
        );

        string[] memory inputs = buildDeployCommand(contractInfo, buildInfoFile, constructorData, defenderOpts);

        Vm.FfiResult memory result = Utils.runAsBashCommand(inputs);
        string memory stdout = string(result.stdout);

        if (result.exitCode != 0) {
            revert(string.concat("Failed to deploy contract ", contractName, ": ", string(result.stderr)));
        }

        string memory deployedAddress = _parseLine("Deployed to address: ", stdout, true);
        return Vm(Utils.CHEATCODE_ADDRESS).parseAddress(deployedAddress);
    }

    function buildDeployCommand(
        ContractInfo memory contractInfo,
        string memory buildInfoFile,
        bytes memory constructorData,
        DefenderOptions memory defenderOpts
    ) internal view returns (string[] memory) {
        Vm vm = Vm(Utils.CHEATCODE_ADDRESS);

        string[] memory inputBuilder = new string[](255);

        uint8 i = 0;

        inputBuilder[i++] = "npx";
        inputBuilder[i++] = string.concat(
            "@openzeppelin/defender-deploy-client-cli@",
            Versions.DEFENDER_DEPLOY_CLIENT_CLI
        );
        inputBuilder[i++] = "deploy";
        inputBuilder[i++] = "--contractName";
        inputBuilder[i++] = contractInfo.shortName;
        inputBuilder[i++] = "--contractPath";
        inputBuilder[i++] = contractInfo.contractPath;
        inputBuilder[i++] = "--chainId";
        inputBuilder[i++] = Strings.toString(block.chainid);
        inputBuilder[i++] = "--buildInfoFile";
        inputBuilder[i++] = buildInfoFile;
        inputBuilder[i++] = "--licenseType";
        inputBuilder[i++] = contractInfo.license;
        if (constructorData.length > 0) {
            inputBuilder[i++] = "--constructorBytecode";
            inputBuilder[i++] = vm.toString(constructorData);
        }
        if (defenderOpts.skipVerifySourceCode) {
            inputBuilder[i++] = "--verifySourceCode";
            inputBuilder[i++] = "false";
        }
        if (!(defenderOpts.relayerId).toSlice().empty()) {
            inputBuilder[i++] = "--relayerId";
            inputBuilder[i++] = defenderOpts.relayerId;
        }
        if (defenderOpts.salt != 0) {
            inputBuilder[i++] = "--salt";
            inputBuilder[i++] = vm.toString(defenderOpts.salt);
        }

        // Create a copy of inputs but with the correct length
        string[] memory inputs = new string[](i);
        for (uint8 j = 0; j < i; j++) {
            inputs[j] = inputBuilder[j];
        }

        return inputs;
    }

    function proposeUpgrade(
        address proxyAddress,
        address proxyAdminAddress,
        address newImplementationAddress,
        string memory newImplementationContractName,
        Options memory opts
    ) internal returns (ProposeUpgradeResponse memory) {
        Vm vm = Vm(Utils.CHEATCODE_ADDRESS);

        string memory outDir = Utils.getOutDir();
        ContractInfo memory contractInfo = Utils.getContractInfo(newImplementationContractName, outDir);

        string[] memory inputs = buildProposeUpgradeCommand(
            proxyAddress,
            proxyAdminAddress,
            newImplementationAddress,
            contractInfo,
            opts
        );

        Vm.FfiResult memory result = Utils.runAsBashCommand(inputs);
        string memory stdout = string(result.stdout);

        if (result.exitCode != 0) {
            revert(
                string.concat(
                    "Failed to propose upgrade for proxy ",
                    vm.toString(proxyAddress),
                    ": ",
                    string(result.stderr)
                )
            );
        }

        return parseProposeUpgradeResponse(stdout);
    }

    function parseProposeUpgradeResponse(string memory stdout) internal pure returns (ProposeUpgradeResponse memory) {
        ProposeUpgradeResponse memory response;
        response.proposalId = _parseLine("Proposal ID: ", stdout, true);
        response.url = _parseLine("Proposal URL: ", stdout, false);
        return response;
    }

    function _parseLine(
        string memory expectedPrefix,
        string memory stdout,
        bool required
    ) private pure returns (string memory) {
        strings.slice memory delim = expectedPrefix.toSlice();
        if (stdout.toSlice().contains(delim)) {
            strings.slice memory slice = stdout.toSlice().copy().find(delim).beyond(delim);
            // Remove any following lines
            if (slice.contains("\n".toSlice())) {
                slice = slice.split("\n".toSlice());
            }
            return slice.toString();
        } else if (required) {
            revert(string.concat("Failed to find line with prefix '", expectedPrefix, "' in output: ", stdout));
        } else {
            return "";
        }
    }

    function buildProposeUpgradeCommand(
        address proxyAddress,
        address proxyAdminAddress,
        address newImplementationAddress,
        ContractInfo memory contractInfo,
        Options memory opts
    ) internal view returns (string[] memory) {
        Vm vm = Vm(Utils.CHEATCODE_ADDRESS);

        string[] memory inputBuilder = new string[](255);

        uint8 i = 0;

        inputBuilder[i++] = "npx";
        inputBuilder[i++] = string.concat(
            "@openzeppelin/defender-deploy-client-cli@",
            Versions.DEFENDER_DEPLOY_CLIENT_CLI
        );
        inputBuilder[i++] = "proposeUpgrade";
        inputBuilder[i++] = "--proxyAddress";
        inputBuilder[i++] = vm.toString(proxyAddress);
        inputBuilder[i++] = "--newImplementationAddress";
        inputBuilder[i++] = vm.toString(newImplementationAddress);
        inputBuilder[i++] = "--chainId";
        inputBuilder[i++] = Strings.toString(block.chainid);
        inputBuilder[i++] = "--contractArtifactFile";
        inputBuilder[i++] = contractInfo.artifactPath;
        if (proxyAdminAddress != address(0)) {
            inputBuilder[i++] = "--proxyAdminAddress";
            inputBuilder[i++] = vm.toString(proxyAdminAddress);
        }
        if (!(opts.defender.upgradeApprovalProcessId).toSlice().empty()) {
            inputBuilder[i++] = "--approvalProcessId";
            inputBuilder[i++] = opts.defender.upgradeApprovalProcessId;
        }

        // Create a copy of inputs but with the correct length
        string[] memory inputs = new string[](i);
        for (uint8 j = 0; j < i; j++) {
            inputs[j] = inputBuilder[j];
        }

        return inputs;
    }

    function getApprovalProcess(string memory command) internal returns (ApprovalProcessResponse memory) {
        string[] memory inputs = buildGetApprovalProcessCommand(command);

        Vm.FfiResult memory result = Utils.runAsBashCommand(inputs);
        string memory stdout = string(result.stdout);

        if (result.exitCode != 0) {
            revert(string.concat("Failed to get approval process: ", string(result.stderr)));
        }

        return parseApprovalProcessResponse(stdout);
    }

    function parseApprovalProcessResponse(string memory stdout) internal pure returns (ApprovalProcessResponse memory) {
        Vm vm = Vm(Utils.CHEATCODE_ADDRESS);

        ApprovalProcessResponse memory response;

        response.approvalProcessId = _parseLine("Approval process ID: ", stdout, true);

        string memory viaString = _parseLine("Via: ", stdout, false);
        if (viaString.toSlice().len() != 0) {
            response.via = vm.parseAddress(viaString);
        }

        response.viaType = _parseLine("Via type: ", stdout, false);

        return response;
    }

    function buildGetApprovalProcessCommand(string memory command) internal view returns (string[] memory) {
        string[] memory inputBuilder = new string[](255);

        uint8 i = 0;

        inputBuilder[i++] = "npx";
        inputBuilder[i++] = string.concat(
            "@openzeppelin/defender-deploy-client-cli@",
            Versions.DEFENDER_DEPLOY_CLIENT_CLI
        );
        inputBuilder[i++] = command;
        inputBuilder[i++] = "--chainId";
        inputBuilder[i++] = Strings.toString(block.chainid);

        // Create a copy of inputs but with the correct length
        string[] memory inputs = new string[](i);
        for (uint8 j = 0; j < i; j++) {
            inputs[j] = inputBuilder[j];
        }

        return inputs;
    }
}
