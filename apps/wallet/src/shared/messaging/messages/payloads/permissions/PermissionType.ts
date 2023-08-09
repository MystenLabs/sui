// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

export const ALL_PERMISSION_TYPES = ['viewAccount', 'suggestTransactions'] as const;
type AllPermissionsType = typeof ALL_PERMISSION_TYPES;
export type PermissionType = AllPermissionsType[number];

/* eslint-disable-next-line @typescript-eslint/no-explicit-any */
export function isValidPermissionTypes(types: any): types is PermissionType[] {
	return (
		Array.isArray(types) &&
		!!types.length &&
		types.every((aType) => ALL_PERMISSION_TYPES.includes(aType))
	);
}
