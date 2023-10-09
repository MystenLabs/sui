// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, test } from 'vitest';

import { computeZkLoginAddressFromSeed } from '../../../src/zklogin/address';

describe('zkLogin address', () => {
	test('generates the correct address', () => {
		expect(
			computeZkLoginAddressFromSeed(
				BigInt('13322897930163218532266430409510394316985274769125667290600321564259466511711'),
				'https://accounts.google.com',
			),
		).toBe('0xf7badc2b245c7f74d7509a4aa357ecf80a29e7713fb4c44b0e7541ec43885ee1');
	});
});
