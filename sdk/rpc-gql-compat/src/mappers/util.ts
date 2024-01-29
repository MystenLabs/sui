// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

export function toShortTypeString<T extends string | null | undefined>(type?: T): T {
	return type?.replace(/0x0+/g, '0x').replace(/,\b/g, ', ') as T;
}

export function isNumericString(value: string) {
	return /^-?\d+$/.test(value);
}
