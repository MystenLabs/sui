// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isValidNamedPackage, isValidNamedType } from '../../utils/move-registry.js';
import type { TransactionDataBuilder } from '../TransactionData.js';

export type NamedPackagesPluginCache = {
	packages: Record<string, string>;
	types: Record<string, string>;
};

const NAME_SEPARATOR = '/';

export type NameResolutionRequest = {
	id: number;
	type: 'package' | 'moveType';
	name: string;
};

/**
 * Looks up all `.move` names in a transaction block.
 * Returns a list of all the names found.
 */
export const findTransactionBlockNames = (
	builder: TransactionDataBuilder,
): { packages: string[]; types: string[] } => {
	const packages: Set<string> = new Set();
	const types: Set<string> = new Set();

	for (const command of builder.commands) {
		if (command.MakeMoveVec?.type) {
			getNamesFromTypeList([command.MakeMoveVec.type]).forEach((type) => {
				types.add(type);
			});
			continue;
		}
		if (!('MoveCall' in command)) continue;
		const tx = command.MoveCall;

		if (!tx) continue;

		const pkg = tx.package.split('::')[0];
		if (pkg.includes(NAME_SEPARATOR)) {
			if (!isValidNamedPackage(pkg)) throw new Error(`Invalid package name: ${pkg}`);
			packages.add(pkg);
		}

		getNamesFromTypeList(tx.typeArguments ?? []).forEach((type) => {
			types.add(type);
		});
	}

	return {
		packages: [...packages],
		types: [...types],
	};
};

/**
 * Returns a list of unique types that include a name
 * from the given list.
 *  */
function getNamesFromTypeList(types: string[]) {
	const names = new Set<string>();
	for (const type of types) {
		if (type.includes(NAME_SEPARATOR)) {
			if (!isValidNamedType(type)) throw new Error(`Invalid type with names: ${type}`);
			names.add(type);
		}
	}
	return [...names];
}

/**
 * Replace all names & types in a transaction block
 * with their resolved names/types.
 */
export const replaceNames = (builder: TransactionDataBuilder, cache: NamedPackagesPluginCache) => {
	for (const command of builder.commands) {
		// Replacements for `MakeMoveVec` commands (that can include types)
		if (command.MakeMoveVec?.type) {
			if (!command.MakeMoveVec.type.includes(NAME_SEPARATOR)) continue;
			if (!cache.types[command.MakeMoveVec.type])
				throw new Error(`No resolution found for type: ${command.MakeMoveVec.type}`);
			command.MakeMoveVec.type = cache.types[command.MakeMoveVec.type];
		}
		// Replacements for `MoveCall` commands (that can include packages & types)
		const tx = command.MoveCall;
		if (!tx) continue;

		const nameParts = tx.package.split('::');
		const name = nameParts[0];

		if (name.includes(NAME_SEPARATOR) && !cache.packages[name])
			throw new Error(`No address found for package: ${name}`);

		nameParts[0] = cache.packages[name];
		tx.package = nameParts.join('::');

		const types = tx.typeArguments;
		if (!types) continue;

		for (let i = 0; i < types.length; i++) {
			if (!types[i].includes(NAME_SEPARATOR)) continue;

			if (!cache.types[types[i]]) throw new Error(`No resolution found for type: ${types[i]}`);
			types[i] = cache.types[types[i]];
		}

		tx.typeArguments = types;
	}
};

export const listToRequests = (
	names: { packages: string[]; types: string[] },
	batchSize: number,
): NameResolutionRequest[][] => {
	const results: NameResolutionRequest[] = [];
	const uniqueNames = deduplicate(names.packages);
	const uniqueTypes = deduplicate(names.types);

	for (const [idx, name] of uniqueNames.entries()) {
		results.push({ id: idx, type: 'package', name } as NameResolutionRequest);
	}
	for (const [idx, type] of uniqueTypes.entries()) {
		results.push({
			id: idx + uniqueNames.length,
			type: 'moveType',
			name: type,
		} as NameResolutionRequest);
	}

	return batch(results, batchSize);
};

const deduplicate = <T>(arr: T[]): T[] => [...new Set(arr)];

const batch = <T>(arr: T[], size: number): T[][] => {
	const batches = [];
	for (let i = 0; i < arr.length; i += size) {
		batches.push(arr.slice(i, i + size));
	}
	return batches;
};
