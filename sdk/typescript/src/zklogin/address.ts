// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { blake2b } from '@noble/hashes/blake2b';
import { bytesToHex } from '@noble/hashes/utils';

import { SIGNATURE_SCHEME_TO_FLAG } from '../cryptography/signature-scheme.js';
import { normalizeSuiAddress, SUI_ADDRESS_LENGTH } from '../utils/index.js';
import { toBigEndianBytes } from './utils.js';

export function computeZkLoginAddressFromSeed(addressSeed: bigint, iss: string) {
	const addressSeedBytesBigEndian = toBigEndianBytes(addressSeed, 32);
	if (iss === 'accounts.google.com') {
		iss = 'https://accounts.google.com';
	}
	const addressParamBytes = new TextEncoder().encode(iss);
	const tmp = new Uint8Array(2 + addressSeedBytesBigEndian.length + addressParamBytes.length);

	tmp.set([SIGNATURE_SCHEME_TO_FLAG.ZkLogin]);
	tmp.set([addressParamBytes.length], 1);
	tmp.set(addressParamBytes, 2);
	tmp.set(addressSeedBytesBigEndian, 2 + addressParamBytes.length);

	return normalizeSuiAddress(
		bytesToHex(blake2b(tmp, { dkLen: 32 })).slice(0, SUI_ADDRESS_LENGTH * 2),
	);
}
