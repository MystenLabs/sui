// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { computeZkLoginAddressFromSeed } from '@mysten/sui.js/zklogin';
import { base64url, decodeJwt } from 'jose';

import { lengthChecks } from './checks.js';
import { JSONProcessor } from './jsonprocessor.js';
import { genAddressSeed } from './utils.js';

export function jwtToAddress(jwt: string, userSalt: string | bigint) {
	const decodedJWT = decodeJwt(jwt);
	if (!decodedJWT.iss) {
		throw new Error('Missing iss');
	}

	const keyClaimName = 'sub';
	const [header, payload] = jwt.split('.');
	const decoded_payload = base64url.decode(payload).toString();
	const processor = new JSONProcessor(decoded_payload);
	const keyClaimDetails = processor.process(keyClaimName); // throws an error if key claim name is not found
	if (typeof keyClaimDetails.value !== 'string') {
		throw new Error('Key claim value must be a string');
	}
	const audDetails = processor.process('aud');
	if (typeof audDetails.value !== 'string') {
		throw new Error('Aud claim value must be a string');
	}

	lengthChecks(header, payload, keyClaimName, processor);

	return computeZkLoginAddress({
		userSalt,
		claimName: keyClaimName,
		claimValue: keyClaimDetails.value,
		aud: audDetails.value,
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
