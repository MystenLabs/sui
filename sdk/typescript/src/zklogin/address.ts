// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { toZkLoginPublicIdentifier } from './publickey.js';

export function computeZkLoginAddressFromSeed(addressSeed: bigint, iss: string) {
	const publicIdentifer = toZkLoginPublicIdentifier(BigInt(addressSeed), iss);
	publicIdentifer.toSuiAddress()
}
