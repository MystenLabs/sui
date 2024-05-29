// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from 'vitest';

import { messageWithIntent } from '../../../src/cryptography/intent';

describe('Intent', () => {
	it('`messageWithIntent()` should combine intent with message correctly', async () => {
		const data = new Uint8Array([0, 0, 0, 5, 72, 101, 108, 108, 111]);
		const intentMessage = messageWithIntent('PersonalMessage', data);

		expect(intentMessage[0]).toEqual(3);
		expect(intentMessage[1]).toEqual(0);
		expect(intentMessage[2]).toEqual(0);
		expect(intentMessage.slice(3)).toEqual(data);
	});
});
