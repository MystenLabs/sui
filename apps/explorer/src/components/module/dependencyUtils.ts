// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import axios from 'axios';
import * as _ from 'lodash';

export const SUI_0x1 = '0x0000000000000000000000000000000000000000000000000000000000000001';
export const SUI_0x2 = '0x0000000000000000000000000000000000000000000000000000000000000002';

interface VersionInfo {
	orgPackageId: string;
	upgradedId: string;
	upgradedVersion: number;
}

export interface SuiPackage {
	network: string;
	packageId: string;
	modules: SuiModule[];
}

export interface SuiModule {
	module: string;
	dependencies: SuiDependency[];
}

export interface SuiDependency {
	orgPackageId: string;
	upgradeCapId: string | null;
	current: SuiPackageVersion;
	latest: SuiPackageVersion;
}

export interface SuiPackageVersion {
	packageId: string | null;
	version: number | null;
}

/**
 * Converts a SuiMoveNormalizedType to string
 * @param network
 * @param packageId
 * @returns null if package not found or dependency package module information
 */
export async function getDependencyVersionInfo(
	network: string,
	packageId: string,
): Promise<SuiPackage | null> {
	const packageObject = await getObject(network, packageId);
	if (!packageObject) {
		return null;
	}

	const moduleAndDependencyIds = getModuleAndDependencyIds(packageObject);
	const versionInfos: VersionInfo[] = await getVersionInfos(packageObject);
	const noneFrameworkVersionInfos = versionInfos.filter(
		(versionInfo) => versionInfo.orgPackageId !== SUI_0x1 && versionInfo.orgPackageId !== SUI_0x2,
	);
	const dependencyOrgPackageIds = noneFrameworkVersionInfos.map((vi) => vi.orgPackageId);
	const previousTransactions = await getPreviousTransactions(network, dependencyOrgPackageIds);
	const upgradeCapIds: string[] = await getUpgradeCapIds(network, previousTransactions);
	const multiGetObjects = await getMultiGetObjects(network, upgradeCapIds);

	const upgradeCapObjects = upgradeCapIds.map((id) => {
		if (id === null) {
			return null;
		}
		const match = multiGetObjects.find((d) => d?.objectId === id);
		return match || null;
	});

	const upgradeCaps = upgradeCapObjects.map((o: any) => {
		if (!o) {
			return {
				id: null,
				package: null,
				policy: null,
				version: null,
			};
		}

		const fields = o.content.fields;
		return {
			id: fields.id.id,
			package: fields.package,
			policy: fields.policy,
			version: fields.version,
		};
	});

	const frameworkObjects = await getMultiGetObjects(network, [SUI_0x1, SUI_0x2]);

	const frameworkDependencies: SuiDependency[] = versionInfos
		.filter((vi) => [SUI_0x1, SUI_0x2].includes(vi.orgPackageId))
		.map((frameworkVersionInfo: VersionInfo): SuiDependency => {
			const frameworkObject =
				frameworkObjects.find((fo: any) => fo.objectId === frameworkVersionInfo.orgPackageId) ||
				null;
			return {
				orgPackageId: frameworkVersionInfo.orgPackageId,
				upgradeCapId: null,
				current: {
					packageId: frameworkVersionInfo.upgradedId,
					version: frameworkVersionInfo.upgradedVersion,
				},
				latest: {
					packageId: frameworkObject.objectId,
					version: _.toNumber(frameworkObject.version) || null,
				},
			};
		});

	const dependencies: SuiDependency[] = dependencyOrgPackageIds.map(
		(dependencyOrgPackageId: string, index: number): SuiDependency => {
			const upgradeCap = upgradeCaps[index];
			const versionInfo = noneFrameworkVersionInfos[index];
			return {
				orgPackageId: dependencyOrgPackageId,
				upgradeCapId: upgradeCap.id,
				current: {
					packageId: versionInfo.upgradedId,
					version: versionInfo.upgradedVersion,
				},
				latest: {
					packageId: upgradeCap.package,
					version: _.toNumber(upgradeCap.version) || null,
				},
			};
		},
	);
	const entireDependencies = [...frameworkDependencies, ...dependencies];

	return {
		network: network,
		packageId: packageId,
		modules: moduleAndDependencyIds.map((dop) => ({
			module: dop.moduleName,
			dependencies: entireDependencies.filter((d) => dop.dependencyIds.includes(d.orgPackageId)),
		})),
	};
}

function getModuleAndDependencyIds(packageObject: any) {
	const disassembled = packageObject.content.disassembled;
	return Object.keys(disassembled).map((moduleName) => ({
		moduleName: moduleName,
		dependencyIds: packageIdsFromDissembled(disassembled[moduleName]),
	}));
}

async function getUpgradeCapIds(network: string, previousTransactions: string[]) {
	const datas = await sui_multiGetTransactionBlocks(network, previousTransactions);

	return datas
		.map((r: any) => r.objectChanges)
		.map((objectChanges: any) => {
			if (!objectChanges) {
				return null;
			}
			const upgradeCapCreated = objectChanges.find(
				(objectChange: any) =>
					objectChange.type === 'created' && objectChange.objectType === '0x2::package::UpgradeCap',
			);
			return upgradeCapCreated?.objectId || null;
		});
}

async function sui_multiGetTransactionBlocks(network: string, previousTransactions: string[]) {
	const res = await axios.post(`https://explorer-rpc.${network}.sui.io/`, {
		jsonrpc: '2.0',
		id: '1',
		method: 'sui_multiGetTransactionBlocks',
		params: [
			previousTransactions,
			{
				showInput: true,
				showRawInput: true,
				showEffects: true,
				showEvents: true,
				showObjectChanges: true,
				showBalanceChanges: true,
			},
		],
	});

	precondition(
		res.status === 200,
		`sui_multiGetTransactionBlocks res error status=${res.status}. tx=${json(
			previousTransactions,
		)}`,
	);

	precondition(
		!res.data.error,
		`sui_multiGetTransactionBlocks res data error. tx=${json(previousTransactions)}, error=${json(
			res.data.error,
		)}`,
	);

	return res.data.result;
}

async function getPreviousTransactions(network: string, dependencyOrgPackageIds: Array<string>) {
	const multiGetObjects_ = await getMultiGetObjects(network, dependencyOrgPackageIds);

	return multiGetObjects_.map((o: any) => o.previousTransaction);
}

async function getMultiGetObjects(network: string, ids: Array<string | null>): Promise<any[]> {
	const res = await axios.post(`https://explorer-rpc.${network}.sui.io/`, {
		jsonrpc: '2.0',
		id: '1',
		method: 'sui_multiGetObjects',
		params: [
			ids,
			{
				showType: true,
				showContent: true,
				showBcs: true,
				showOwner: true,
				showPreviousTransaction: true,
				showStorageRebate: true,
				showDisplay: true,
			},
		],
	});

	precondition(
		res.status === 200,
		`sui_multiGetObjects res error status=${res.status}. tx=${json(ids)}`,
	);

	precondition(
		!res.data.error,
		`sui_getObject res data error. id=${json(ids)}, error=${json(res.data.error)}`,
	);

	return res.data.result.map((r: any) => r.data);
}

async function getVersionInfos(packageObject: any): Promise<VersionInfo[]> {
	const linkageTable = packageObject.bcs.linkageTable;
	return Object.keys(linkageTable).map(
		(objectId): VersionInfo => ({
			orgPackageId: objectId,
			upgradedId: linkageTable[objectId].upgraded_id,
			upgradedVersion: linkageTable[objectId].upgraded_version,
		}),
	);
}

async function getObject(network: string, packageId: string) {
	const res = await axios.post(`https://explorer-rpc.${network}.sui.io/`, {
		jsonrpc: '2.0',
		id: '1',
		method: 'sui_getObject',
		params: [
			packageId,
			{
				showType: true,
				showContent: true,
				showBcs: true,
				showOwner: true,
				showPreviousTransaction: true,
				showStorageRebate: true,
				showDisplay: true,
			},
		],
	});

	precondition(
		res.status === 200,
		`sui_multiGetObjects res error status=${res.status}. tx=${json(packageId)}`,
	);

	precondition(
		!res.data.error,
		`sui_getObject res data error. id=${json(packageId)}, error=${json(res.data.error)}`,
	);

	return res.data.result.data;
}

function packageIdsFromDissembled(disassembled: string) {
	const lines = disassembled.split('\n');
	const useLines = lines.filter((line) => line.startsWith('use'));
	const dependencyPackageIds = useLines.map(
		(useLine) => '0x' + useLine.slice(4, useLine.indexOf('::')),
	);

	return _.uniq(dependencyPackageIds);
}

function precondition(condition: any, msg?: string): asserts condition {
	if (!condition) {
		throw new Error(msg);
	}
}

function json(unknown: unknown) {
	try {
		return JSON.stringify(unknown, null, 2);
	} catch (e) {
		console.error(`json stringify error`);
		return 'json stringify error';
	}
}
