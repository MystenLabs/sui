// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { hexToBytes } from '@noble/hashes/utils';

export function toBufferBE(num: bigint, width: number) {
	const hex = num.toString(16);
	return hexToBytes(hex.padStart(width * 2, '0').slice(-width * 2));
}
