// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, test } from 'vitest';

import { bcs } from '../../../src/bcs';
import { deriveDynamicFieldID } from '../../../src/utils';

describe('dynamic field utils', () => {
	test('deriveDynamicFieldID', () => {
		const parentId = '0xbef336120c90707eb387d72dde9c0e9f6fea37b9f02b1ba8de271c64ad7b6db0';
		const key = bcs.u64().serialize(0).toBytes();
		const typeTag = 'u64';

		const result = deriveDynamicFieldID(parentId, typeTag, key);

		expect(result).toBe('0x6552700b707460a3f48d8348f7531c4bba7a9546b4cf445ba265ad2ba06b1bb3');
	});
});
