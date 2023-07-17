// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { it, describe, expect } from 'vitest';
import { TypeTagSerializer } from '../../../../src/builder/type-tag-serializer.js';

describe('parseFromStr', () => {
	it('parses nested struct type from a string', () => {
		const typeStr =
			'0x2::balance::Supply<0x72de5feb63c0ab6ed1cda7e5b367f3d0a999add7::amm::LP<0x2::sui::SUI, 0xfee024a3c0c03ada5cdbda7d0e8b68802e6dec80::example_coin::EXAMPLE_COIN>>';
		const act = TypeTagSerializer.parseFromStr(typeStr);
		const exp = {
			struct: {
				address: '0x2',
				module: 'balance',
				name: 'Supply',
				typeParams: [
					{
						struct: {
							address: '0x72de5feb63c0ab6ed1cda7e5b367f3d0a999add7',
							module: 'amm',
							name: 'LP',
							typeParams: [
								{
									struct: {
										address: '0x2',
										module: 'sui',
										name: 'SUI',
										typeParams: [],
									},
								},
								{
									struct: {
										address: '0xfee024a3c0c03ada5cdbda7d0e8b68802e6dec80',
										module: 'example_coin',
										name: 'EXAMPLE_COIN',
										typeParams: [],
									},
								},
							],
						},
					},
				],
			},
		};
		expect(act).toEqual(exp);
	});

	it('parses non parametrized struct type from a string', () => {
		const typeStr = '0x72de5feb63c0ab6ed1cda7e5b367f3d0a999add7::foo::FOO';
		const act = TypeTagSerializer.parseFromStr(typeStr);
		const exp = {
			struct: {
				address: '0x72de5feb63c0ab6ed1cda7e5b367f3d0a999add7',
				module: 'foo',
				name: 'FOO',
				typeParams: [],
			},
		};
		expect(act).toEqual(exp);
	});
});

describe('tagToString', () => {
	it('converts nested struct type to a string', () => {
		const type = {
			struct: {
				address: '0x2',
				module: 'balance',
				name: 'Supply',
				typeParams: [
					{
						struct: {
							address: '0x72de5feb63c0ab6ed1cda7e5b367f3d0a999add7',
							module: 'amm',
							name: 'LP',
							typeParams: [
								{
									struct: {
										address: '0x2',
										module: 'sui',
										name: 'SUI',
										typeParams: [],
									},
								},
								{
									struct: {
										address: '0xfee024a3c0c03ada5cdbda7d0e8b68802e6dec80',
										module: 'example_coin',
										name: 'EXAMPLE_COIN',
										typeParams: [],
									},
								},
							],
						},
					},
				],
			},
		};
		const act = TypeTagSerializer.tagToString(type);
		const exp =
			'0x2::balance::Supply<0x72de5feb63c0ab6ed1cda7e5b367f3d0a999add7::amm::LP<0x2::sui::SUI, 0xfee024a3c0c03ada5cdbda7d0e8b68802e6dec80::example_coin::EXAMPLE_COIN>>';
		expect(act).toEqual(exp);
	});
});
