// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

export const ALL_PERMISSION_TYPES = [
    'viewAccount',
    'suggestTransactions',
] as const;
type AllPermissionsType = typeof ALL_PERMISSION_TYPES;
export type PermissionType = AllPermissionsType[number];
