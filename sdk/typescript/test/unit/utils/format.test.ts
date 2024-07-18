// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, test } from 'vitest';

import { formatAmount } from '../../../src/utils';

describe('format', () => {
	test('formatAmount', () => {
		expect(formatAmount(undefined)).toMatch('--');
		expect(formatAmount(null)).toMatch('--');
		expect(formatAmount('12345')).toMatch('12.34 K');
		expect(formatAmount('12345678')).toMatch('12.34 M');
		expect(formatAmount('12345678910')).toMatch('12.34 B');
	});

	// @todo Also test formatAddress() and formatDigest().
});
