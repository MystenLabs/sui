// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect } from 'vitest';
import { BCS, getSuiMoveConfig } from '../src/index';
import { serde } from './utils';

describe('BCS: Nested temp object', () => {
	it('should support object as a type', () => {
		const bcs = new BCS(getSuiMoveConfig());
		const value = { name: { boop: 'beep', beep: '100' } };

		bcs.registerStructType('Beep', {
			name: {
				boop: BCS.STRING,
				beep: BCS.U64,
			},
		});

		expect(serde(bcs, 'Beep', value)).toEqual(value);
	});

	it('should support enum invariant as an object', () => {
		const bcs = new BCS(getSuiMoveConfig());
		const value = {
			user: {
				name: 'Bob',
				age: 20,
			},
		};

		bcs.registerEnumType('AccountType', {
			system: null,
			user: {
				name: BCS.STRING,
				age: BCS.U8,
			},
		});

		expect(serde(bcs, 'AccountType', value)).toEqual(value);
	});

	it('should support a nested schema', () => {
		const bcs = new BCS(getSuiMoveConfig());
		const value = {
			some: {
				account: {
					user: 'Bob',
					age: 20,
				},
				meta: {
					status: {
						active: true,
					},
				},
			},
		};

		bcs.registerEnumType('Option', {
			none: null,
			some: {
				account: {
					user: BCS.STRING,
					age: BCS.U8,
				},
				meta: {
					status: {
						active: BCS.BOOL,
					},
				},
			},
		});

		expect(serde(bcs, 'Option', value)).toEqual(value);
	});
});
