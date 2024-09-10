// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { toB58 } from '@mysten/bcs';
import { describe, expect, it } from 'vitest';

import { Inputs, Transaction } from '../../src/transactions';

describe('V1 JSON serialization', () => {
	it('can serialize and deserialize transactions', async () => {
		const tx = new Transaction();

		tx.moveCall({
			target: '0x2::foo::bar',
			arguments: [
				tx.object('0x123'),
				tx.object(
					Inputs.ReceivingRef({
						objectId: '1',
						version: '123',
						digest: toB58(new Uint8Array(32).fill(0x1)),
					}),
				),
				tx.object(
					Inputs.SharedObjectRef({
						objectId: '2',
						mutable: true,
						initialSharedVersion: '123',
					}),
				),
				tx.object(
					Inputs.ObjectRef({
						objectId: '3',
						version: '123',
						digest: toB58(new Uint8Array(32).fill(0x1)),
					}),
				),
				tx.pure.address('0x2'),
			],
		});

		const jsonv2 = await tx.toJSON();
		const jsonv1 = JSON.parse(tx.serialize());

		expect(jsonv1).toMatchInlineSnapshot(`
			{
			  "expiration": null,
			  "gasConfig": {},
			  "inputs": [
			    {
			      "index": 0,
			      "kind": "Input",
			      "type": "object",
			      "value": "0x0000000000000000000000000000000000000000000000000000000000000123",
			    },
			    {
			      "index": 1,
			      "kind": "Input",
			      "type": "object",
			      "value": {
			        "Object": {
			          "Receiving": {
			            "digest": "4vJ9JU1bJJE96FWSJKvHsmmFADCg4gpZQff4P3bkLKi",
			            "objectId": "0x0000000000000000000000000000000000000000000000000000000000000001",
			            "version": "123",
			          },
			        },
			      },
			    },
			    {
			      "index": 2,
			      "kind": "Input",
			      "type": "object",
			      "value": {
			        "Object": {
			          "Shared": {
			            "initialSharedVersion": "123",
			            "mutable": true,
			            "objectId": "0x0000000000000000000000000000000000000000000000000000000000000002",
			          },
			        },
			      },
			    },
			    {
			      "index": 3,
			      "kind": "Input",
			      "type": "object",
			      "value": {
			        "Object": {
			          "ImmOrOwned": {
			            "digest": "4vJ9JU1bJJE96FWSJKvHsmmFADCg4gpZQff4P3bkLKi",
			            "objectId": "0x0000000000000000000000000000000000000000000000000000000000000003",
			            "version": "123",
			          },
			        },
			      },
			    },
			    {
			      "index": 4,
			      "kind": "Input",
			      "type": "pure",
			      "value": {
			        "Pure": [
			          0,
			          0,
			          0,
			          0,
			          0,
			          0,
			          0,
			          0,
			          0,
			          0,
			          0,
			          0,
			          0,
			          0,
			          0,
			          0,
			          0,
			          0,
			          0,
			          0,
			          0,
			          0,
			          0,
			          0,
			          0,
			          0,
			          0,
			          0,
			          0,
			          0,
			          0,
			          2,
			        ],
			      },
			    },
			  ],
			  "transactions": [
			    {
			      "arguments": [
			        {
			          "index": 0,
			          "kind": "Input",
			          "type": "object",
			          "value": "0x0000000000000000000000000000000000000000000000000000000000000123",
			        },
			        {
			          "index": 1,
			          "kind": "Input",
			          "type": "object",
			          "value": {
			            "Object": {
			              "Receiving": {
			                "digest": "4vJ9JU1bJJE96FWSJKvHsmmFADCg4gpZQff4P3bkLKi",
			                "objectId": "0x0000000000000000000000000000000000000000000000000000000000000001",
			                "version": "123",
			              },
			            },
			          },
			        },
			        {
			          "index": 2,
			          "kind": "Input",
			          "type": "object",
			          "value": {
			            "Object": {
			              "Shared": {
			                "initialSharedVersion": "123",
			                "mutable": true,
			                "objectId": "0x0000000000000000000000000000000000000000000000000000000000000002",
			              },
			            },
			          },
			        },
			        {
			          "index": 3,
			          "kind": "Input",
			          "type": "object",
			          "value": {
			            "Object": {
			              "ImmOrOwned": {
			                "digest": "4vJ9JU1bJJE96FWSJKvHsmmFADCg4gpZQff4P3bkLKi",
			                "objectId": "0x0000000000000000000000000000000000000000000000000000000000000003",
			                "version": "123",
			              },
			            },
			          },
			        },
			        {
			          "index": 4,
			          "kind": "Input",
			          "type": "pure",
			          "value": {
			            "Pure": [
			              0,
			              0,
			              0,
			              0,
			              0,
			              0,
			              0,
			              0,
			              0,
			              0,
			              0,
			              0,
			              0,
			              0,
			              0,
			              0,
			              0,
			              0,
			              0,
			              0,
			              0,
			              0,
			              0,
			              0,
			              0,
			              0,
			              0,
			              0,
			              0,
			              0,
			              0,
			              2,
			            ],
			          },
			        },
			      ],
			      "kind": "MoveCall",
			      "target": "0x0000000000000000000000000000000000000000000000000000000000000002::foo::bar",
			      "typeArguments": [],
			    },
			  ],
			  "version": 1,
			}
		`);

		const tx2 = Transaction.from(JSON.stringify(jsonv1));

		expect(await tx2.toJSON()).toEqual(jsonv2);

		expect(jsonv2).toMatchInlineSnapshot(`
			"{
			  "version": 2,
			  "sender": null,
			  "expiration": null,
			  "gasData": {
			    "budget": null,
			    "price": null,
			    "owner": null,
			    "payment": null
			  },
			  "inputs": [
			    {
			      "UnresolvedObject": {
			        "objectId": "0x0000000000000000000000000000000000000000000000000000000000000123"
			      }
			    },
			    {
			      "Object": {
			        "Receiving": {
			          "objectId": "0x0000000000000000000000000000000000000000000000000000000000000001",
			          "version": "123",
			          "digest": "4vJ9JU1bJJE96FWSJKvHsmmFADCg4gpZQff4P3bkLKi"
			        }
			      }
			    },
			    {
			      "Object": {
			        "SharedObject": {
			          "objectId": "0x0000000000000000000000000000000000000000000000000000000000000002",
			          "initialSharedVersion": "123",
			          "mutable": true
			        }
			      }
			    },
			    {
			      "Object": {
			        "ImmOrOwnedObject": {
			          "objectId": "0x0000000000000000000000000000000000000000000000000000000000000003",
			          "version": "123",
			          "digest": "4vJ9JU1bJJE96FWSJKvHsmmFADCg4gpZQff4P3bkLKi"
			        }
			      }
			    },
			    {
			      "Pure": {
			        "bytes": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAI="
			      }
			    }
			  ],
			  "commands": [
			    {
			      "MoveCall": {
			        "package": "0x0000000000000000000000000000000000000000000000000000000000000002",
			        "module": "foo",
			        "function": "bar",
			        "typeArguments": [],
			        "arguments": [
			          {
			            "Input": 0
			          },
			          {
			            "Input": 1
			          },
			          {
			            "Input": 2
			          },
			          {
			            "Input": 3
			          },
			          {
			            "Input": 4
			          }
			        ]
			      }
			    }
			  ]
			}"
		`);
	});
});
