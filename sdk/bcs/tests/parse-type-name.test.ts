// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect } from 'vitest';

import { BCS, getSuiMoveConfig } from '../src';

describe('parseTypeName', () => {
	it('parses nested struct type from a string', () => {
		const bcs = new BCS(getSuiMoveConfig());

		const type =
			'0x5::foo::Foo<0x5::bar::Bar, 0x6::amm::LP<0x2::sui::SUI, 0x7::example_coin::EXAMPLE_COIN>>';
		expect(bcs.parseTypeName(type)).toEqual({
			name: '0x5::foo::Foo',
			params: ['0x5::bar::Bar', '0x6::amm::LP<0x2::sui::SUI, 0x7::example_coin::EXAMPLE_COIN>'],
		});
	});
});
