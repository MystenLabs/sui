// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { bcs } from '@mysten/bcs';
import { describe, expect, test } from 'vitest';

import { Transaction } from '../../src/transactions';

describe('tx.pure serialization', () => {
	test('serialized pure values', () => {
		const tx = new Transaction();

		tx.pure.u8(1);
		tx.pure.u16(1);
		tx.pure.u32(1);
		tx.pure.u64(1n);
		tx.pure.u128(1n);
		tx.pure.u256(1n);
		tx.pure.bool(true);
		tx.pure.string('foo');
		tx.pure.address('0x2');
		tx.pure.id('0x2');
		tx.pure(bcs.vector(bcs.u8()).serialize([1, 2, 3]));
		tx.pure(bcs.option(bcs.u8()).serialize(1));
		tx.pure(bcs.option(bcs.u8()).serialize(null));
		tx.pure(
			bcs.option(bcs.vector(bcs.vector(bcs.option(bcs.u8())))).serialize([
				[1, null, 3],
				[4, null, 6],
			]),
		);

		const tx2 = new Transaction();

		tx2.pure('u8', 1);
		tx2.pure('u16', 1);
		tx2.pure('u32', 1);
		tx2.pure('u64', 1n);
		tx2.pure('u128', 1n);
		tx2.pure('u256', 1n);
		tx2.pure('bool', true);
		tx2.pure('string', 'foo');
		tx2.pure('address', '0x2');
		tx2.pure('id', '0x2');
		tx2.pure('vector<u8>', [1, 2, 3]);
		tx2.pure('option<u8>', 1);
		tx2.pure('option<u8>', null);
		tx2.pure('option<vector<vector<option<u8>>>>', [
			[1, null, 3],
			[4, null, 6],
		]);

		expect(tx.getData().inputs).toEqual(tx2.getData().inputs);

		expect(tx.getData().inputs).toMatchInlineSnapshot(`
			[
			  {
			    "$kind": "Pure",
			    "Pure": {
			      "bytes": "AQ==",
			    },
			  },
			  {
			    "$kind": "Pure",
			    "Pure": {
			      "bytes": "AQA=",
			    },
			  },
			  {
			    "$kind": "Pure",
			    "Pure": {
			      "bytes": "AQAAAA==",
			    },
			  },
			  {
			    "$kind": "Pure",
			    "Pure": {
			      "bytes": "AQAAAAAAAAA=",
			    },
			  },
			  {
			    "$kind": "Pure",
			    "Pure": {
			      "bytes": "AQAAAAAAAAAAAAAAAAAAAA==",
			    },
			  },
			  {
			    "$kind": "Pure",
			    "Pure": {
			      "bytes": "AQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
			    },
			  },
			  {
			    "$kind": "Pure",
			    "Pure": {
			      "bytes": "AQ==",
			    },
			  },
			  {
			    "$kind": "Pure",
			    "Pure": {
			      "bytes": "A2Zvbw==",
			    },
			  },
			  {
			    "$kind": "Pure",
			    "Pure": {
			      "bytes": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAI=",
			    },
			  },
			  {
			    "$kind": "Pure",
			    "Pure": {
			      "bytes": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAI=",
			    },
			  },
			  {
			    "$kind": "Pure",
			    "Pure": {
			      "bytes": "AwECAw==",
			    },
			  },
			  {
			    "$kind": "Pure",
			    "Pure": {
			      "bytes": "AQE=",
			    },
			  },
			  {
			    "$kind": "Pure",
			    "Pure": {
			      "bytes": "AA==",
			    },
			  },
			  {
			    "$kind": "Pure",
			    "Pure": {
			      "bytes": "AQIDAQEAAQMDAQQAAQY=",
			    },
			  },
			]
		`);
	});
});
