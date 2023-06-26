// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from 'vitest';
import { SUI_ADDRESS_LENGTH } from '../../typescript/src';
import { BCS, getSuiMoveConfig } from './../src/index';

describe('BCS: Serde', () => {
	it('should serialize primitives in both directions', () => {
		const bcs = new BCS(getSuiMoveConfig());

		expect(serde(bcs, 'u8', '0').toString(10)).toEqual('0');
		expect(serde(bcs, 'u8', '200').toString(10)).toEqual('200');
		expect(serde(bcs, 'u8', '255').toString(10)).toEqual('255');

		expect(serde(bcs, 'u16', '10000').toString(10)).toEqual('10000');
		expect(serde(bcs, 'u32', '10000').toString(10)).toEqual('10000');
		expect(serde(bcs, 'u128', '154386538175093611946334810').toString(10)).toEqual(
			'154386538175093611946334810',
		);

		expect(
			serde(bcs, 'u256', '154386538175093611946334810000000000000000000000122').toString(10),
		).toEqual('154386538175093611946334810000000000000000000000122');

		expect(bcs.ser('u256', '100000').toString('hex')).toEqual(
			'a086010000000000000000000000000000000000000000000000000000000000',
		);

		expect(serde(bcs, 'u64', '1000').toString(10)).toEqual('1000');
		expect(serde(bcs, 'u128', '1000').toString(10)).toEqual('1000');
		expect(serde(bcs, 'u256', '1000').toString(10)).toEqual('1000');

		expect(serde(bcs, 'bool', true)).toEqual(true);
		expect(serde(bcs, 'bool', false)).toEqual(false);

		expect(
			serde(bcs, 'address', '0x000000000000000000000000e3edac2c684ddbba5ad1a2b90fb361100b2094af'),
		).toEqual('000000000000000000000000e3edac2c684ddbba5ad1a2b90fb361100b2094af');
	});

	it('should serde structs', () => {
		let bcs = new BCS(getSuiMoveConfig());

		bcs.registerAddressType('address', SUI_ADDRESS_LENGTH, 'hex');
		bcs.registerStructType('Beep', { id: 'address', value: 'u64' });

		let bytes = bcs
			.ser('Beep', {
				id: '0x00000000000000000000000045aacd9ed90a5a8e211502ac3fa898a3819f23b2',
				value: 10000000,
			})
			.toBytes();
		let struct = bcs.de('Beep', bytes);

		expect(struct.id).toEqual('00000000000000000000000045aacd9ed90a5a8e211502ac3fa898a3819f23b2');
		expect(struct.value.toString(10)).toEqual('10000000');
	});

	it('should serde enums', () => {
		let bcs = new BCS(getSuiMoveConfig());
		bcs.registerAddressType('address', SUI_ADDRESS_LENGTH, 'hex');
		bcs.registerEnumType('Enum', {
			with_value: 'address',
			no_value: null,
		});

		let addr = 'bb967ddbebfee8c40d8fdd2c24cb02452834cd3a7061d18564448f900eb9e66d';

		expect(addr).toEqual(
			bcs.de('Enum', bcs.ser('Enum', { with_value: addr }).toBytes()).with_value,
		);
		expect(
			'no_value' in bcs.de('Enum', bcs.ser('Enum', { no_value: null }).toBytes()),
		).toBeTruthy();
	});

	it('should serde vectors natively', () => {
		let bcs = new BCS(getSuiMoveConfig());

		{
			let value = ['0', '255', '100'];
			expect(serde(bcs, 'vector<u8>', value).map((e) => e.toString(10))).toEqual(value);
		}

		{
			let value = ['100000', '555555555', '1123123', '0', '1214124124214'];
			expect(serde(bcs, 'vector<u64>', value).map((e) => e.toString(10))).toEqual(value);
		}

		{
			let value = ['100000', '555555555', '1123123', '0', '1214124124214'];
			expect(serde(bcs, 'vector<u128>', value).map((e) => e.toString(10))).toEqual(value);
		}

		{
			let value = [true, false, false, true, false];
			expect(serde(bcs, 'vector<bool>', value)).toEqual(value);
		}

		{
			let value = [
				'000000000000000000000000e3edac2c684ddbba5ad1a2b90fb361100b2094af',
				'0000000000000000000000000000000000000000000000000000000000000001',
				'0000000000000000000000000000000000000000000000000000000000000002',
				'000000000000000000000000c0ffeec0ffeec0ffeec0ffeec0ffeec0ffee1337',
			];

			expect(serde(bcs, 'vector<address>', value)).toEqual(value);
		}

		{
			let value = [
				[true, false, true, true],
				[true, true, false, true],
				[false, true, true, true],
				[true, true, true, false],
			];

			expect(serde(bcs, 'vector<vector<bool>>', value)).toEqual(value);
		}
	});

	it('should structs and nested enums', () => {
		let bcs = new BCS(getSuiMoveConfig());

		bcs.registerStructType('User', { age: 'u64', name: 'string' });
		bcs.registerStructType('Coin<T>', { balance: 'Balance<T>' });
		bcs.registerStructType('Balance<T>', { value: 'u64' });

		bcs.registerStructType('Container<T>', {
			owner: 'address',
			is_active: 'bool',
			item: 'T',
		});

		{
			let value = { age: '30', name: 'Bob' };
			expect(serde(bcs, 'User', value).age.toString(10)).toEqual(value.age);
			expect(serde(bcs, 'User', value).name).toEqual(value.name);
		}

		{
			let value = {
				owner: '0000000000000000000000000000000000000000000000000000000000000001',
				is_active: true,
				item: { balance: { value: '10000' } },
			};

			// Deep Nested Generic!
			let result = serde(bcs, 'Container<Coin<Balance<T>>>', value);

			expect(result.owner).toEqual(value.owner);
			expect(result.is_active).toEqual(value.is_active);
			expect(result.item.balance.value.toString(10)).toEqual(value.item.balance.value);
		}
	});

	it('should serde SuiObjectRef', () => {
		const bcs = new BCS(getSuiMoveConfig());
		bcs.registerStructType('SuiObjectRef', {
			objectId: 'address',
			version: 'u64',
			digest: 'ObjectDigest',
		});

		// console.log('base58', toB64('1Bhh3pU9gLXZhoVxkr5wyg9sX6'));

		bcs.registerAlias('ObjectDigest', BCS.STRING);

		const value = {
			objectId: '5443700000000000000000000000000000000000000000000000000000000000',
			version: '9180',
			digest: 'hahahahahaha',
		};

		expect(serde(bcs, 'SuiObjectRef', value)).toEqual(value);
	});
});

function serde(bcs: BCS, type, data) {
	let ser = bcs.ser(type, data).toString('hex');
	let de = bcs.de(type, ser, 'hex');
	return de;
}
