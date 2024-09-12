// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/** The pattern to find an optionally versioned name */
const NAME_PATTERN = /^([a-z0-9]+(?:-[a-z0-9]+)*)@([a-z0-9]+(?:-[a-z0-9]+)*)(?:\/v(\d+))?$/;

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
		if (t.includes('@') && !isValidNamedPackage(t)) return false;
	}
	// TODO: Add `isValidStructTag` check once
	// it's generally introduced.
	return true;
};
