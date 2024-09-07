// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { TransactionDataBuilder } from '../transactions/TransactionData.js';

/** The pattern to find an optionally versioned name */
const NAME_PATTERN = /^([a-z0-9]+(?:-[a-z0-9]+)*)@([a-z0-9]+(?:-[a-z0-9]+)*)(?:\/v(\d+))?$/;
const NAME_SEPARATOR = '@';

export type NameResolutionRequest = {
	id: number;
	type: 'package' | 'moveType';
	name: string;
};

export type NamedPackagesPluginCache = {
	packages: Record<string, string>;
	types: Record<string, string>;
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
		if (!('MoveCall' in command)) continue;
		const tx = command.MoveCall;

		if (!tx) continue;

		const pkg = tx.package.split('::')[0];
		if (pkg.includes(NAME_SEPARATOR)) {
			if (!isValidNamedPackage(pkg)) throw new Error(`Invalid package name: ${pkg}`);
			packages.add(pkg);
		}

		for (const type of tx.typeArguments ?? []) {
			if (type.includes(NAME_SEPARATOR)) {
				if (!isValidNamedType(type)) throw new Error(`Invalid type with names: ${type}`);
				types.add(type);
			}
		}
	}

	return {
		packages: [...packages],
		types: [...types],
	};
};

/**
 * Replace all names & types in a transaction block
 * with their resolved names/types.
 */
export const replaceNames = (builder: TransactionDataBuilder, cache: NamedPackagesPluginCache) => {
	for (const command of builder.commands) {
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

export const isValidNamedPackage = (name: string): boolean => {
	return NAME_PATTERN.test(name);
};

/**
 * Checks if a type contains valid named packages.
 * This DOES NOT check if the type is a valid Move type.
 */
export const isValidNamedType = (type: string): boolean => {
	// split our type by all possible type delimeters.
	const splitType = type.split(/::|<|>|,/);
	for (const t of splitType) {
		if (t.includes(NAME_SEPARATOR) && !isValidNamedPackage(t)) return false;
	}
	return true;
};

const deduplicate = <T>(arr: T[]): T[] => [...new Set(arr)];

const batch = <T>(arr: T[], size: number): T[][] => {
	const batches = [];
	for (let i = 0; i < arr.length; i += size) {
		batches.push(arr.slice(i, i + size));
	}
	return batches;
};
