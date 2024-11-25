// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { expect, test } from 'vitest';

import { poseidonHash } from '../../../src/zklogin';
import { BN254_FIELD_SIZE } from '../../../src/zklogin/poseidon';

test('can hash single input', () => {
	const result = poseidonHash([123]);
	expect(result).toBeTypeOf('bigint');
});

test('can hash multiple inputs', () => {
	const result = poseidonHash([1, 2, 3, 4, 5]);
	expect(result).toBeTypeOf('bigint');
});

test('throws error for invalid input', () => {
	expect(() => poseidonHash([-1])).toThrowError('Element -1 not in the BN254 field');
});

test('throws error for invalid input greater than BN254_FIELD_SIZE', () => {
	expect(() => poseidonHash([BN254_FIELD_SIZE])).toThrowError(
		'Element 21888242871839275222246405745257275088548364400416034343698204186575808495617 not in the BN254 field',
	);
});
