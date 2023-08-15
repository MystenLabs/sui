// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { bytesToHex } from '@noble/hashes/utils';
import { blake2b } from '@noble/hashes/blake2b';
import { SIGNATURE_SCHEME_TO_FLAG } from '../../typescript/src/cryptography/signature.js';
import { SUI_ADDRESS_LENGTH, normalizeSuiAddress } from '../../typescript/src/utils/index.js';
import { zkBcs } from './bcs.js';
import { decodeJwt } from 'jose';
import { genAddressSeed, toBufferBE } from './utils.js';

export function jwtToAddress(jwt: string, userPin: bigint) {
	const decodedJWT = decodeJwt(jwt);
	if (
		!decodedJWT.sub ||
		!decodedJWT.iss ||
		!decodedJWT.aud ||
		!decodedJWT.email ||
		typeof decodedJWT.email !== 'string'
	) {
		throw new Error('Missing jwt data');
	}

	if (Array.isArray(decodedJWT.aud)) {
		throw new Error('Not supported aud. Aud is an array, string was expected.');
	}

	return computeZkAddress({
		userPin,
		claimName: 'sub',
		claimValue: decodedJWT.sub,
		aud: decodedJWT.aud,
		iss: decodedJWT.iss,
	});
}

export interface ComputeZKAddressOptions {
	claimName: string;
	claimValue: string;
	userPin: bigint;
	iss: string;
	aud: string;
}

export function computeZkAddress({
	claimName,
	claimValue,
	iss,
	aud,
	userPin,
}: ComputeZKAddressOptions) {
	const addressSeedBytesBigEndian = toBufferBE(genAddressSeed(userPin, claimName, claimValue), 32);
	const addressParamBytes = zkBcs.ser('AddressParams', { iss, aud }).toBytes();

	const tmp = new Uint8Array(1 + addressSeedBytesBigEndian.length + addressParamBytes.length);
	tmp.set([SIGNATURE_SCHEME_TO_FLAG.Zk]);
	tmp.set(addressParamBytes, 1);
	tmp.set(addressSeedBytesBigEndian, 1 + addressParamBytes.length);

	return normalizeSuiAddress(
		bytesToHex(blake2b(tmp, { dkLen: 32 })).slice(0, SUI_ADDRESS_LENGTH * 2),
	);
}
