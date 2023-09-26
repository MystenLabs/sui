// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/**
 * Utility for compile-time exhaustiveness checking.
 */
export function assertUnreachable(value: never): never {
	throw new Error(`ERROR! Encountered an unexpected value: ${value}`);
}
