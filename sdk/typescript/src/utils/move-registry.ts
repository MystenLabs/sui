// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isValidSuiNSName } from './suins.js';

/** The pattern to find an optionally versioned name */
const NAME_PATTERN = /^([a-z0-9]+(?:-[a-z0-9]+)*)$/;
/** The pattern for a valid version number */
const VERSION_REGEX = /^\d+$/;
/** The maximum size for an app */
const MAX_APP_SIZE = 64;
/** The separator for the name */
const NAME_SEPARATOR = '/';

export const isValidNamedPackage = (name: string): boolean => {
	const parts = name.split(NAME_SEPARATOR);
	// The name has to have 2 parts (without-version), or 3 parts (with version).
	if (parts.length < 2 || parts.length > 3) return false;

	const [org, app, version] = parts; // split by {org} {app} {optional version}

	// If the version exists, it must be a number.
	if (version !== undefined && !VERSION_REGEX.test(version)) return false;
	// Check if the org is a valid SuiNS name.
	if (!isValidSuiNSName(org)) return false;

	// Check if the app is a valid name.
	return NAME_PATTERN.test(app) && app.length < MAX_APP_SIZE;
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
	// TODO: Add `isValidStructTag` check once it's introduced.
	return true;
};
