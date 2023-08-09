// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect } from 'vitest';
import { BCS, getSuiMoveConfig } from '../src/index';
import { serde } from './utils';

describe('BCS: Array type', () => {
	it.skip('should support destructured type name in ser/de', () => {
		const bcs = new BCS(getSuiMoveConfig());
		const values = ['this is a string'];

		expect(serde(bcs, ['vector', BCS.STRING], values)).toEqual(values);
	});

	it('should support destructured type name in struct', () => {
		const bcs = new BCS(getSuiMoveConfig());
		const value = {
			name: 'Bob',
			role: 'Admin',
			meta: {
				lastLogin: '23 Feb',
				isActive: false,
			},
		};

		bcs.registerStructType('Metadata', {
			lastLogin: BCS.STRING,
			isActive: BCS.BOOL,
		});

		bcs.registerStructType(['User', 'T'], {
			name: BCS.STRING,
			role: BCS.STRING,
			meta: 'T',
		});

		expect(serde(bcs, ['User', 'Metadata'], value)).toEqual(value);
	});

	it('should support destructured type name in enum', () => {
		const bcs = new BCS(getSuiMoveConfig());
		const values = { some: ['this is a string'] };

		bcs.registerEnumType(['Option', 'T'], {
			none: null,
			some: 'T',
		});

		expect(serde(bcs, ['Option', ['vector', 'string']], values)).toEqual(values);
	});

	it('should solve nested generic issue', () => {
		const bcs = new BCS(getSuiMoveConfig());
		const value = {
			contents: {
				content_one: { key: 'A', value: 'B' },
				content_two: { key: 'C', value: 'D' },
			},
		};

		bcs.registerStructType(['Entry', 'K', 'V'], {
			key: 'K',
			value: 'V',
		});

		bcs.registerStructType(['Wrapper', 'A', 'B'], {
			content_one: 'A',
			content_two: 'B',
		});

		bcs.registerStructType(['VecMap', 'K', 'V'], {
			contents: ['Wrapper', ['Entry', 'K', 'V'], ['Entry', 'V', 'K']],
		});

		expect(serde(bcs, ['VecMap', 'string', 'string'], value)).toEqual(value);
	});

	// More complicated invariant of the test case above
	it('should support arrays in global generics', () => {
		const bcs = new BCS(getSuiMoveConfig());
		bcs.registerEnumType(['Option', 'T'], {
			none: null,
			some: 'T',
		});
		const value = {
			contents: {
				content_one: { key: { some: 'A' }, value: ['B'] },
				content_two: { key: [], value: { none: true } },
			},
		};

		bcs.registerStructType(['Entry', 'K', 'V'], {
			key: 'K',
			value: 'V',
		});

		bcs.registerStructType(['Wrapper', 'A', 'B'], {
			content_one: 'A',
			content_two: 'B',
		});

		bcs.registerStructType(['VecMap', 'K', 'V'], {
			contents: ['Wrapper', ['Entry', 'K', 'V'], ['Entry', 'V', 'K']],
		});

		expect(serde(bcs, ['VecMap', ['Option', 'string'], ['vector', 'string']], value)).toEqual(
			value,
		);
	});
});
