// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { computeZkLoginAddressFromSeed } from '@mysten/sui.js/zklogin';
import { decodeJwt } from 'jose';

import { genAddressSeed } from './utils.js';

export function jwtToAddress(jwt: string, userSalt: bigint) {
	const decodedJWT = decodeJwt(jwt);
	if (!decodedJWT.sub || !decodedJWT.iss || !decodedJWT.aud) {
		throw new Error('Missing jwt data');
	}

	if (Array.isArray(decodedJWT.aud)) {
		throw new Error('Not supported aud. Aud is an array, string was expected.');
	}

	return computeZkAddress({
		userSalt,
		claimName: 'sub',
		claimValue: decodedJWT.sub,
		aud: decodedJWT.aud,
		iss: decodedJWT.iss,
	});
}

export interface ComputeZKAddressOptions {
	claimName: string;
	claimValue: string;
	userSalt: bigint;
	iss: string;
	aud: string;
}

export function computeZkAddress({
	claimName,
	claimValue,
	iss,
	aud,
	userSalt,
}: ComputeZKAddressOptions) {
	return computeZkLoginAddressFromSeed(genAddressSeed(userSalt, claimName, claimValue, aud), iss);
}
