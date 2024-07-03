// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { computeZkLoginAddressFromSeed } from '@mysten/sui/zklogin';
import { decodeJwt } from 'jose';

import { genAddressSeed } from './utils.js';

export const MAX_HEADER_LEN_B64 = 248;
export const MAX_PADDED_UNSIGNED_JWT_LEN = 64 * 25;

export function lengthChecks(jwt: string) {
	const [header, payload] = jwt.split('.');
	/// Is the header small enough
	if (header.length > MAX_HEADER_LEN_B64) {
		throw new Error(`Header is too long`);
	}

	/// Is the combined length of (header, payload, SHA2 padding) small enough?
	// unsigned_jwt = header + '.' + payload;
	const L = (header.length + 1 + payload.length) * 8;
	const K = (512 + 448 - ((L % 512) + 1)) % 512;

	// The SHA2 padding is 1 followed by K zeros, followed by the length of the message
	const padded_unsigned_jwt_len = (L + 1 + K + 64) / 8;

	// The padded unsigned JWT must be less than the max_padded_unsigned_jwt_len
	if (padded_unsigned_jwt_len > MAX_PADDED_UNSIGNED_JWT_LEN) {
		throw new Error(`JWT is too long`);
	}
}

export function jwtToAddress(jwt: string, userSalt: string | bigint) {
	lengthChecks(jwt);

	const decodedJWT = decodeJwt(jwt);
	if (!decodedJWT.sub || !decodedJWT.iss || !decodedJWT.aud) {
		throw new Error('Missing jwt data');
	}

	if (Array.isArray(decodedJWT.aud)) {
		throw new Error('Not supported aud. Aud is an array, string was expected.');
	}

	return computeZkLoginAddress({
		userSalt,
		claimName: 'sub',
		claimValue: decodedJWT.sub,
		aud: decodedJWT.aud,
		iss: decodedJWT.iss,
	});
}

export interface ComputeZkLoginAddressOptions {
	claimName: string;
	claimValue: string;
	userSalt: string | bigint;
	iss: string;
	aud: string;
}

export function computeZkLoginAddress({
	claimName,
	claimValue,
	iss,
	aud,
	userSalt,
}: ComputeZkLoginAddressOptions) {
	return computeZkLoginAddressFromSeed(genAddressSeed(userSalt, claimName, claimValue, aud), iss);
}
