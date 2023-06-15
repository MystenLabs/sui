// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect } from 'vitest';
import { BCS, getSuiMoveConfig } from '../src/index';
import { serde } from './utils';

describe('BCS: Aliases', () => {
	it('should support type aliases', () => {
		const bcs = new BCS(getSuiMoveConfig());
		const value = 'this is a string';

		bcs.registerAlias('MyString', BCS.STRING);
		expect(serde(bcs, 'MyString', value)).toEqual(value);
	});

	it('should support recursive definitions in structs', () => {
		const bcs = new BCS(getSuiMoveConfig());
		const value = { name: 'Billy' };

		bcs.registerAlias('UserName', BCS.STRING);
		expect(serde(bcs, { name: 'UserName' }, value)).toEqual(value);
	});

	it('should spot recursive definitions', () => {
		const bcs = new BCS(getSuiMoveConfig());
		const value = 'this is a string';

		bcs.registerAlias('MyString', BCS.STRING);
		bcs.registerAlias(BCS.STRING, 'MyString');

		let error = null;
		try {
			serde(bcs, 'MyString', value);
		} catch (e) {
			error = e;
		}

		expect(error).toBeInstanceOf(Error);
	});
});
