// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, test } from 'vitest';

import { toZkLoginPublicIdentifier } from '../../../src/zklogin';
import {
	computeZkLoginAddressFromSeed,
	jwtToAddress,
	lengthChecks,
	MAX_HEADER_LEN_B64,
	MAX_PADDED_UNSIGNED_JWT_LEN,
} from '../../../src/zklogin/address';

describe('zkLogin address', () => {
	test('generates the correct address', () => {
		expect(
			computeZkLoginAddressFromSeed(
				BigInt('13322897930163218532266430409510394316985274769125667290600321564259466511711'),
				'https://accounts.google.com',
			),
		).toBe('0xf7badc2b245c7f74d7509a4aa357ecf80a29e7713fb4c44b0e7541ec43885ee1');
	});

	test('generates the correct address for a seed with leading zeros', () => {
		expect(
			computeZkLoginAddressFromSeed(
				BigInt('380704556853533152350240698167704405529973457670972223618755249929828551006'),
				'https://accounts.google.com',
			),
		).toBe('0xbd8b8ed42d90aebc71518385d8a899af14cef8b5a171c380434dd6f5bbfe7bf3');
	});

	test('computeZkLoginAddressFromSeed matches ZkLoginPublicIdentifier.toSuiAddress() for legacy addresses', () => {
		const seed = BigInt(
			'380704556853533152350240698167704405529973457670972223618755249929828551006',
		);
		const iss = 'https://accounts.google.com';
		expect(computeZkLoginAddressFromSeed(seed, iss)).toEqual(
			toZkLoginPublicIdentifier(seed, iss, { legacyAddress: true }).toSuiAddress(),
		);
	});

	test('computeZkLoginAddressFromSeed matches ZkLoginPublicIdentifier.toSuiAddress() for non-legacy addresses', () => {
		const seed = BigInt(
			'380704556853533152350240698167704405529973457670972223618755249929828551006',
		);
		const iss = 'https://accounts.google.com';
		expect(computeZkLoginAddressFromSeed(seed, iss, false)).toEqual(
			toZkLoginPublicIdentifier(seed, iss).toSuiAddress(),
		);
	});

	test('a valid JWT should not throw an error', () => {
		const jwt =
			'eyJraWQiOiJzdWkta2V5LWlkIiwidHlwIjoiSldUIiwiYWxnIjoiUlMyNTYifQ.eyJzdWIiOiI4YzJkN2Q2Ni04N2FmLTQxZmEtYjZmYy02M2U4YmI3MWZhYjQiLCJhdWQiOiJ0ZXN0IiwibmJmIjoxNjk3NDY1NDQ1LCJpc3MiOiJodHRwczovL29hdXRoLnN1aS5pbyIsImV4cCI6MTY5NzU1MTg0NSwibm9uY2UiOiJoVFBwZ0Y3WEFLYlczN3JFVVM2cEVWWnFtb0kifQ.';
		const userSalt = '248191903847969014646285995941615069143';
		const address = jwtToAddress(jwt, userSalt);
		expect(address).toBe('0x22cebcf68a9d75d508d50d553dd6bae378ef51177a3a6325b749e57e3ba237d6');
	});

	test('should return the same address for both google iss', () => {
		/**
		 * {
		 * "iss": "https://accounts.google.com",
		 * "sub": "1234567890",
		 * "aud": "1234567890.apps.googleusercontent.com",
		 * "exp": 1697551845,
		 * "iat": 1697465445
		 * }
		 */
		const jwt1 =
			'eyJhbGciOiJSUzI1NiIsImtpZCI6InN1aS1rZXktaWQiLCJ0eXAiOiJKV1QifQ.eyJpc3MiOiJodHRwczovL2FjY291bnRzLmdvb2dsZS5jb20iLCJzdWIiOiIxMjM0NTY3ODkwIiwiYXVkIjoiMTIzNDU2Nzg5MC5hcHBzLmdvb2dsZXVzZXJjb250ZW50LmNvbSIsImV4cCI6MTY5NzU1MTg0NSwiaWF0IjoxNjk3NDY1NDQ1fQ.';
		/**
		 * {
		 * "iss": "accounts.google.com",
		 * "sub": "1234567890",
		 * "aud": "1234567890.apps.googleusercontent.com",
		 * "exp": 1697551845,
		 * "iat": 1697465445
		 * }
		 */
		const jwt2 =
			'eyJhbGciOiJSUzI1NiIsImtpZCI6InN1aS1rZXktaWQiLCJ0eXAiOiJKV1QifQ.eyJpc3MiOiJhY2NvdW50cy5nb29nbGUuY29tIiwic3ViIjoiMTIzNDU2Nzg5MCIsImF1ZCI6IjEyMzQ1Njc4OTAuYXBwcy5nb29nbGV1c2VyY29udGVudC5jb20iLCJleHAiOjE2OTc1NTE4NDUsImlhdCI6MTY5NzQ2NTQ0NX0.';

		expect(jwtToAddress(jwt1, '0')).toBe(jwtToAddress(jwt2, '0'));
	});

	test('lengthChecks: if header is too long, should throw an error', () => {
		const header = 'a'.repeat(MAX_HEADER_LEN_B64 + 1);
		const jwt = `${header}.`;
		expect(() => lengthChecks(jwt)).toThrow(`Header is too long`);
	});

	test('lengthChecks: if jwt is too long, should throw an error', () => {
		// Note: It should also fail for lengths slightly smaller than MAX_PADDED_UNSIGNED_JWT_LEN due to the SHA2 padding.
		const jwt = '.' + 'a'.repeat(MAX_PADDED_UNSIGNED_JWT_LEN);
		expect(() => lengthChecks(jwt)).toThrow(`JWT is too long`);
	});
});
