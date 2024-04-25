// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SuiClient } from '@mysten/sui.js/client';
import { Ed25519Keypair } from '@mysten/sui.js/keypairs/ed25519';
import { TransactionBlock } from '@mysten/sui.js/transactions';
import { fromB64 } from '@mysten/sui.js/utils';
import { expect, it } from 'vitest';

import { initLoaderIfNeeded } from './gen/_framework/init-source';
import { structClassLoaderSource } from './gen/_framework/loader';
import { extractType, vector } from './gen/_framework/reified';
import {
	createBar,
	createFoo,
	createSpecial,
	createSpecialAsGenerics,
	createSpecialInVectors,
	createWithGenericField,
	createWithTwoGenerics,
} from './gen/examples/fixture/functions';
import {
	Bar,
	Dummy,
	Foo,
	WithGenericField,
	WithSpecialTypes,
	WithSpecialTypesAsGenerics,
	WithSpecialTypesInVectors,
	WithTwoGenerics,
} from './gen/examples/fixture/structs';
import { StructFromOtherModule } from './gen/examples/other-module/structs';
import { string } from './gen/move-stdlib/ascii/functions';
import { String as AsciiString } from './gen/move-stdlib/ascii/structs';
import { none, some } from './gen/move-stdlib/option/functions';
import { Option } from './gen/move-stdlib/option/structs';
import { utf8 } from './gen/move-stdlib/string/functions';
import { String as Utf8String } from './gen/move-stdlib/string/structs';
import { zero } from './gen/sui/balance/functions';
import { Balance } from './gen/sui/balance/structs';
import { idFromAddress, new_ as newUid } from './gen/sui/object/functions';
import { ID, UID } from './gen/sui/object/structs';
import { SUI } from './gen/sui/sui/structs';
import { newUnsafeFromBytes } from './gen/sui/url/functions';
import { Url } from './gen/sui/url/structs';

const keypair = Ed25519Keypair.fromSecretKey(
	fromB64('AMVT58FaLF2tJtg/g8X2z1/vG0FvNn0jvRu9X2Wl8F+u').slice(1),
); // address: 0x8becfafb14c111fc08adee6cc9afa95a863d1bf133f796626eec353f98ea8507

const client = new SuiClient({
	url: 'https://fullnode.testnet.sui.io:443/',
});

it('creates and decodes an object with object as type param', async () => {
	const txb = new TransactionBlock();

	const T = Bar.$typeName;

	const genericVecNested = [
		createWithTwoGenerics(txb, [T, 'u8'], {
			genericField1: createBar(txb, 100n),
			genericField2: 1,
		}),
	];

	const twoGenericsNested = createWithTwoGenerics(
		txb,
		[T, `${WithTwoGenerics.$typeName}<u8, u8>`],
		{
			genericField1: createBar(txb, 100n),
			genericField2: createWithTwoGenerics(txb, ['u8', 'u8'], {
				genericField1: 1,
				genericField2: 2,
			}),
		},
	);

	const twoGenericsReifiedNested = createWithTwoGenerics(
		txb,
		[Bar.$typeName, `${WithTwoGenerics.$typeName}<u8, u8>`],
		{
			genericField1: createBar(txb, 100n),
			genericField2: createWithTwoGenerics(txb, ['u8', 'u8'], {
				genericField1: 1,
				genericField2: 2,
			}),
		},
	);

	const twoGenericsNestedVec = [
		createWithTwoGenerics(txb, [Bar.$typeName, `vector<${WithTwoGenerics.$typeName}<${T}, u8>>`], {
			genericField1: createBar(txb, 100n),
			genericField2: [
				createWithTwoGenerics(txb, [Bar.$typeName, 'u8'], {
					genericField1: createBar(txb, 100n),
					genericField2: 1,
				}),
			],
		}),
	];

	createFoo(txb, [T, Bar.$typeName], {
		generic: createBar(txb, 100n),
		reifiedPrimitiveVec: [1n, 2n, 3n],
		reifiedObjectVec: [createBar(txb, 100n)],
		genericVec: [createBar(txb, 100n)],
		genericVecNested,
		twoGenerics: createWithTwoGenerics(txb, [T, Bar.$typeName], {
			genericField1: createBar(txb, 100n),
			genericField2: createBar(txb, 100n),
		}),
		twoGenericsReifiedPrimitive: createWithTwoGenerics(txb, ['u16', 'u64'], {
			genericField1: 1,
			genericField2: 2n,
		}),
		twoGenericsReifiedObject: createWithTwoGenerics(txb, [Bar.$typeName, Bar.$typeName], {
			genericField1: createBar(txb, 100n),
			genericField2: createBar(txb, 100n),
		}),
		twoGenericsNested,
		twoGenericsReifiedNested,
		twoGenericsNestedVec,
		objRef: createBar(txb, 100n),
	});

	const res = await client.signAndExecuteTransactionBlock({
		signer: keypair,
		transactionBlock: txb,
		options: {
			showEffects: true,
		},
	});

	const id = res.effects!.created![0].reference.objectId;

	const foo = await client.getObject({
		id,
		options: {
			showBcs: true,
			showContent: true,
		},
	});

	if (foo.data?.bcs?.dataType !== 'moveObject' || foo.data?.content?.dataType !== 'moveObject') {
		throw new Error(`not a moveObject`);
	}

	const exp = Foo.r(Bar.reified()).new({
		id,
		generic: Bar.r.new({ value: 100n }),
		reifiedPrimitiveVec: [1n, 2n, 3n],
		reifiedObjectVec: [Bar.r.new({ value: 100n })],
		genericVec: [Bar.r.new({ value: 100n })],
		genericVecNested: [
			WithTwoGenerics.r(Bar.reified(), 'u8').new({
				genericField1: Bar.r.new({ value: 100n }),
				genericField2: 1,
			}),
		],
		twoGenerics: WithTwoGenerics.r(Bar.reified(), Bar.reified()).new({
			genericField1: Bar.r.new({ value: 100n }),
			genericField2: Bar.r.new({ value: 100n }),
		}),
		twoGenericsReifiedPrimitive: WithTwoGenerics.r('u16', 'u64').new({
			genericField1: 1,
			genericField2: 2n,
		}),
		twoGenericsReifiedObject: WithTwoGenerics.r(Bar.reified(), Bar.reified()).new({
			genericField1: Bar.r.new({ value: 100n }),
			genericField2: Bar.r.new({ value: 100n }),
		}),
		twoGenericsNested: WithTwoGenerics.r(Bar.reified(), WithTwoGenerics.reified('u8', 'u8')).new({
			genericField1: Bar.r.new({ value: 100n }),
			genericField2: WithTwoGenerics.r('u8', 'u8').new({
				genericField1: 1,
				genericField2: 2,
			}),
		}),
		twoGenericsReifiedNested: WithTwoGenerics.r(
			Bar.reified(),
			WithTwoGenerics.reified('u8', 'u8'),
		).new({
			genericField1: Bar.r.new({ value: 100n }),
			genericField2: WithTwoGenerics.r('u8', 'u8').new({
				genericField1: 1,
				genericField2: 2,
			}),
		}),
		twoGenericsNestedVec: [
			WithTwoGenerics.r(Bar.reified(), vector(WithTwoGenerics.reified(Bar.reified(), 'u8'))).new({
				genericField1: Bar.r.new({ value: 100n }),
				genericField2: [
					WithTwoGenerics.r(Bar.reified(), 'u8').new({
						genericField1: Bar.r.new({ value: 100n }),
						genericField2: 1,
					}),
				],
			}),
		],
		dummy: Dummy.r.new({ dummyField: false }),
		other: StructFromOtherModule.r.new({ dummyField: false }),
	});

	const de = Foo.fromBcs(Bar.reified(), fromB64(foo.data.bcs.bcsBytes));

	expect(de).toEqual(exp);
	expect(Foo.fromFieldsWithTypes(Bar.reified(), foo.data.content)).toEqual(exp);
	expect(Foo.fromSuiParsedData(Bar.reified(), foo.data.content)).toEqual(exp);
	expect(await Foo.fetch(client, Bar.reified(), id)).toEqual(exp);
	expect(Foo.fromJSON(Bar.reified(), de.toJSON())).toEqual(exp);
});

it('creates and decodes Foo with vector of objects as type param', async () => {
	const txb = new TransactionBlock();

	const T = `vector<${Bar.$typeName}>`;
	const reifiedT = vector(Bar.reified());

	function createT(txb: TransactionBlock, value: bigint) {
		return txb.makeMoveVec({
			objects: [createBar(txb, value)],
			type: Bar.$typeName,
		});
	}

	const genericVecNested = [
		createWithTwoGenerics(txb, [T, 'u8'], {
			genericField1: [createBar(txb, 100n)],
			genericField2: 1,
		}),
	];

	const twoGenericsNested = createWithTwoGenerics(
		txb,
		[T, `${WithTwoGenerics.$typeName}<u8, u8>`],
		{
			genericField1: [createBar(txb, 100n)],
			genericField2: createWithTwoGenerics(txb, ['u8', 'u8'], {
				genericField1: 1,
				genericField2: 2,
			}),
		},
	);

	const twoGenericsReifiedNested = createWithTwoGenerics(
		txb,
		[Bar.$typeName, `${WithTwoGenerics.$typeName}<u8, u8>`],
		{
			genericField1: createBar(txb, 100n),
			genericField2: createWithTwoGenerics(txb, ['u8', 'u8'], {
				genericField1: 1,
				genericField2: 2,
			}),
		},
	);

	const twoGenericsNestedVec = [
		createWithTwoGenerics(txb, [Bar.$typeName, `vector<${WithTwoGenerics.$typeName}<${T}, u8>>`], {
			genericField1: createBar(txb, 100n),
			genericField2: [
				createWithTwoGenerics(txb, [T, 'u8'], {
					genericField1: createT(txb, 100n), // or [createBar(txb, 100n)],
					genericField2: 1,
				}),
			],
		}),
	];

	createFoo(txb, [T, Bar.$typeName], {
		generic: createT(txb, 100n), // or [createBar(txb, 100n)]
		reifiedPrimitiveVec: [1n, 2n, 3n],
		reifiedObjectVec: [createBar(txb, 100n)],
		genericVec: [createT(txb, 100n)],
		genericVecNested,
		twoGenerics: createWithTwoGenerics(txb, [T, Bar.$typeName], {
			genericField1: [createBar(txb, 100n), createBar(txb, 100n)],
			genericField2: createBar(txb, 100n),
		}),
		twoGenericsReifiedPrimitive: createWithTwoGenerics(txb, ['u16', 'u64'], {
			genericField1: 1,
			genericField2: 2n,
		}),
		twoGenericsReifiedObject: createWithTwoGenerics(txb, [Bar.$typeName, Bar.$typeName], {
			genericField1: createBar(txb, 100n),
			genericField2: createBar(txb, 100n),
		}),
		twoGenericsNested,
		twoGenericsReifiedNested,
		twoGenericsNestedVec,
		objRef: createBar(txb, 100n),
	});

	const res = await client.signAndExecuteTransactionBlock({
		signer: keypair,
		transactionBlock: txb,
		options: {
			showEffects: true,
		},
	});

	const id = res.effects!.created![0].reference.objectId;

	const foo = await client.getObject({
		id,
		options: {
			showBcs: true,
			showContent: true,
		},
	});

	if (foo.data?.bcs?.dataType !== 'moveObject' || foo.data?.content?.dataType !== 'moveObject') {
		throw new Error(`not a moveObject`);
	}

	const exp = Foo.r(reifiedT).new({
		id: id,
		generic: [Bar.r.new({ value: 100n })],
		reifiedPrimitiveVec: [1n, 2n, 3n],
		reifiedObjectVec: [Bar.r.new({ value: 100n })],
		genericVec: [[Bar.r.new({ value: 100n })]],
		genericVecNested: [
			WithTwoGenerics.r(reifiedT, 'u8').new({
				genericField1: [Bar.r.new({ value: 100n })],
				genericField2: 1,
			}),
		],
		twoGenerics: WithTwoGenerics.r(reifiedT, Bar.reified()).new({
			genericField1: [Bar.r.new({ value: 100n }), Bar.r.new({ value: 100n })],
			genericField2: Bar.r.new({ value: 100n }),
		}),
		twoGenericsReifiedPrimitive: WithTwoGenerics.r('u16', 'u64').new({
			genericField1: 1,
			genericField2: 2n,
		}),
		twoGenericsReifiedObject: WithTwoGenerics.r(Bar.reified(), Bar.reified()).new({
			genericField1: Bar.r.new({ value: 100n }),
			genericField2: Bar.r.new({ value: 100n }),
		}),
		twoGenericsNested: WithTwoGenerics.r(reifiedT, WithTwoGenerics.reified('u8', 'u8')).new({
			genericField1: [Bar.r.new({ value: 100n })],
			genericField2: WithTwoGenerics.r('u8', 'u8').new({
				genericField1: 1,
				genericField2: 2,
			}),
		}),
		twoGenericsReifiedNested: WithTwoGenerics.r(
			Bar.reified(),
			WithTwoGenerics.reified('u8', 'u8'),
		).new({
			genericField1: Bar.r.new({ value: 100n }),
			genericField2: WithTwoGenerics.r('u8', 'u8').new({
				genericField1: 1,
				genericField2: 2,
			}),
		}),
		twoGenericsNestedVec: [
			WithTwoGenerics.r(Bar.reified(), vector(WithTwoGenerics.reified(reifiedT, 'u8'))).new({
				genericField1: Bar.r.new({ value: 100n }),
				genericField2: [
					WithTwoGenerics.r(reifiedT, 'u8').new({
						genericField1: [Bar.r.new({ value: 100n })],
						genericField2: 1,
					}),
				],
			}),
		],
		dummy: Dummy.r.new({ dummyField: false }),
		other: StructFromOtherModule.r.new({ dummyField: false }),
	});

	const de = Foo.fromBcs(reifiedT, fromB64(foo.data.bcs.bcsBytes));

	expect(de).toEqual(exp);

	expect(Foo.fromFieldsWithTypes(reifiedT, foo.data.content)).toEqual(exp);
	expect(Foo.fromJSON(reifiedT, de.toJSON())).toEqual(exp);
});

it('decodes special-cased types correctly', async () => {
	const txb = new TransactionBlock();

	const encoder = new TextEncoder();

	const typeArgs = ['0x2::sui::SUI', 'u64'] as [string, string];
	const reifiedArgs = [SUI.p, 'u64'] as const;

	createSpecial(txb, typeArgs, {
		string: utf8(txb, Array.from(encoder.encode('string'))),
		asciiString: string(txb, Array.from(encoder.encode('ascii'))),
		url: newUnsafeFromBytes(txb, Array.from(encoder.encode('https://example.com'))),
		idField: idFromAddress(txb, 'faf60f9f9d1f6c490dce8673c1371b9df456e0c183f38524e5f78d959ea559a5'),
		uid: newUid(txb),
		balance: zero(txb, '0x2::sui::SUI'),
		option: some(txb, 'u64', 100n),
		optionObj: some(txb, Bar.$typeName, createBar(txb, 100n)),
		optionNone: none(txb, 'u64'),
		balanceGeneric: zero(txb, '0x2::sui::SUI'),
		optionGeneric: some(txb, 'u64', 200n),
		optionGenericNone: none(txb, 'u64'),
	});

	const res = await client.signAndExecuteTransactionBlock({
		signer: keypair,
		transactionBlock: txb,
		options: {
			showEffects: true,
		},
	});

	const id = res.effects!.created![0].reference.objectId;

	const obj = await client.getObject({
		id,
		options: {
			showBcs: true,
			showContent: true,
		},
	});

	if (obj.data?.bcs?.dataType !== 'moveObject' || obj.data?.content?.dataType !== 'moveObject') {
		throw new Error(`not a moveObject`);
	}

	const fromBcs = WithSpecialTypes.r(...reifiedArgs).fromBcs(fromB64(obj.data.bcs.bcsBytes));
	const fromFieldsWithTypes = WithSpecialTypes.r(...reifiedArgs).fromFieldsWithTypes(
		obj.data.content,
	);

	const uid = (obj.data.content.fields as { uid: { id: string } }).uid.id;

	const exp = WithSpecialTypes.r(...reifiedArgs).new({
		id,
		string: 'string',
		asciiString: 'ascii',
		url: 'https://example.com',
		idField: '0xfaf60f9f9d1f6c490dce8673c1371b9df456e0c183f38524e5f78d959ea559a5',
		uid,
		balance: Balance.r(SUI.p).new({ value: 0n }),
		option: 100n,
		optionObj: Bar.r.new({ value: 100n }),
		optionNone: null,
		balanceGeneric: Balance.r(SUI.p).new({ value: 0n }),
		optionGeneric: 200n,
		optionGenericNone: null,
	});

	expect(fromFieldsWithTypes).toEqual(exp);
	expect(fromBcs).toEqual(exp);
	expect(WithSpecialTypes.r(...reifiedArgs).fromJSON(exp.toJSON())).toEqual(exp);
});

it('decodes special-cased types as generics correctly', async () => {
	const txb = new TransactionBlock();

	const encoder = new TextEncoder();

	const typeArgs = [
		'0x1::string::String',
		'0x1::ascii::String',
		'0x2::url::Url',
		'0x2::object::ID',
		'0x2::object::UID',
		'0x2::balance::Balance<0x2::sui::SUI>',
		'0x1::option::Option<u64>',
		'0x1::option::Option<u64>',
	] as [string, string, string, string, string, string, string, string];
	const reifiedArgs = [
		Utf8String.reified(),
		AsciiString.reified(),
		Url.reified(),
		ID.reified(),
		UID.reified(),
		Balance.reified(SUI.p),
		Option.reified('u64'),
		Option.reified('u64'),
	] as const;

	createSpecialAsGenerics(txb, typeArgs, {
		string: utf8(txb, Array.from(encoder.encode('string'))),
		asciiString: string(txb, Array.from(encoder.encode('ascii'))),
		url: newUnsafeFromBytes(txb, Array.from(encoder.encode('https://example.com'))),
		idField: idFromAddress(txb, 'faf60f9f9d1f6c490dce8673c1371b9df456e0c183f38524e5f78d959ea559a5'),
		uid: newUid(txb),
		balance: zero(txb, '0x2::sui::SUI'),
		option: some(txb, 'u64', 100n),
		optionNone: none(txb, 'u64'),
	});

	const res = await client.signAndExecuteTransactionBlock({
		signer: keypair,
		transactionBlock: txb,
		options: {
			showEffects: true,
		},
	});

	const id = res.effects!.created![0].reference.objectId;

	const obj = await client.getObject({
		id,
		options: {
			showBcs: true,
			showContent: true,
		},
	});

	if (obj.data?.bcs?.dataType !== 'moveObject' || obj.data?.content?.dataType !== 'moveObject') {
		throw new Error(`not a moveObject`);
	}

	const uid = (obj.data.content.fields as { uid: { id: string } }).uid.id;

	const fromBcs = WithSpecialTypesAsGenerics.r(...reifiedArgs).fromBcs(
		fromB64(obj.data.bcs.bcsBytes),
	);
	const fromFieldsWithTypes = WithSpecialTypesAsGenerics.r(...reifiedArgs).fromFieldsWithTypes(
		obj.data.content,
	);

	const exp = WithSpecialTypesAsGenerics.r(...reifiedArgs).new({
		id,
		string: 'string',
		asciiString: 'ascii',
		url: 'https://example.com',
		idField: '0xfaf60f9f9d1f6c490dce8673c1371b9df456e0c183f38524e5f78d959ea559a5',
		uid,
		balance: Balance.r(SUI.p).new({ value: 0n }),
		option: 100n,
		optionNone: null,
	});

	expect(fromBcs).toEqual(exp);
	expect(fromFieldsWithTypes).toEqual(exp);
	expect(WithSpecialTypesAsGenerics.r(...reifiedArgs).fromJSON(exp.toJSON())).toEqual(exp);
});

it('calls function correctly when special types are used', async () => {
	const txb = new TransactionBlock();

	const encoder = new TextEncoder();

	const reifiedArgs = [
		SUI.p,
		vector(Option.reified(Option.reified(vector(vector('u64'))))),
	] as const;

	createSpecial(
		txb,
		['0x2::sui::SUI', 'vector<0x1::option::Option<0x1::option::Option<vector<vector<u64>>>>>'],
		{
			string: 'string',
			asciiString: 'ascii',
			url: newUnsafeFromBytes(txb, Array.from(encoder.encode('https://example.com'))),
			idField: idFromAddress(
				txb,
				'0xfaf60f9f9d1f6c490dce8673c1371b9df456e0c183f38524e5f78d959ea559a5',
			),
			uid: newUid(txb),
			balance: zero(txb, '0x2::sui::SUI'),
			option: 100n,
			optionObj: some(txb, Bar.$typeName, createBar(txb, 100n)),
			optionNone: null,
			balanceGeneric: zero(txb, '0x2::sui::SUI'),
			optionGeneric: [[[200n, 300n]], null, [[400n, 500n]]],
			optionGenericNone: null,
		},
	);

	const res = await client.signAndExecuteTransactionBlock({
		signer: keypair,
		transactionBlock: txb,
		options: {
			showEffects: true,
		},
	});

	const id = res.effects!.created![0].reference.objectId;

	const obj = await client.getObject({
		id,
		options: {
			showBcs: true,
			showContent: true,
		},
	});

	if (obj.data?.bcs?.dataType !== 'moveObject' || obj.data?.content?.dataType !== 'moveObject') {
		throw new Error(`not a moveObject`);
	}

	expect(WithSpecialTypes.fromFieldsWithTypes([SUI.p, reifiedArgs[1]], obj.data.content)).toEqual(
		WithSpecialTypes.r(SUI.p, reifiedArgs[1]).new({
			id,
			string: 'string',
			asciiString: 'ascii',
			url: 'https://example.com',
			idField: '0xfaf60f9f9d1f6c490dce8673c1371b9df456e0c183f38524e5f78d959ea559a5',
			uid: (obj.data.content.fields as { uid: { id: string } }).uid.id,
			balance: Balance.r(SUI.p).new({ value: 0n }),
			option: 100n,
			optionObj: Bar.r.new({ value: 100n }),
			optionNone: null,
			balanceGeneric: Balance.r(SUI.p).new({ value: 0n }),
			optionGeneric: [[[200n, 300n]], null, [[400n, 500n]]],
			optionGenericNone: null,
		}),
	);
});

it('calls function correctly when special types are used as generics', async () => {
	const txb = new TransactionBlock();

	const encoder = new TextEncoder();

	const reifiedArgs = [
		Utf8String.reified(),
		AsciiString.reified(),
		Url.reified(),
		ID.reified(),
		UID.reified(),
		Balance.reified(SUI.p),
		Option.reified(vector(Option.reified('u64'))),
		Option.reified('u64'),
	] as const;

	createSpecialAsGenerics(
		txb,
		[
			'0x1::string::String',
			'0x1::ascii::String',
			'0x2::url::Url',
			'0x2::object::ID',
			'0x2::object::UID',
			'0x2::balance::Balance<0x2::sui::SUI>',
			'0x1::option::Option<vector<0x1::option::Option<u64>>>',
			'0x1::option::Option<u64>',
		],
		{
			string: 'string',
			asciiString: 'ascii',
			url: newUnsafeFromBytes(txb, Array.from(encoder.encode('https://example.com'))),
			idField: idFromAddress(
				txb,
				'0xfaf60f9f9d1f6c490dce8673c1371b9df456e0c183f38524e5f78d959ea559a5',
			),
			uid: newUid(txb),
			balance: zero(txb, '0x2::sui::SUI'),
			option: [5n, null, 3n],
			optionNone: null,
		},
	);

	const res = await client.signAndExecuteTransactionBlock({
		signer: keypair,
		transactionBlock: txb,
		options: {
			showEffects: true,
		},
	});

	const id = res.effects!.created![0].reference.objectId;

	const obj = await client.getObject({
		id,
		options: {
			showBcs: true,
			showContent: true,
		},
	});

	if (obj.data?.bcs?.dataType !== 'moveObject' || obj.data?.content?.dataType !== 'moveObject') {
		throw new Error(`not a moveObject`);
	}

	expect(
		WithSpecialTypesAsGenerics.r(...reifiedArgs).fromFieldsWithTypes(obj.data.content),
	).toEqual(
		WithSpecialTypesAsGenerics.r(...reifiedArgs).new({
			id,
			string: 'string',
			asciiString: 'ascii',
			url: 'https://example.com',
			idField: '0xfaf60f9f9d1f6c490dce8673c1371b9df456e0c183f38524e5f78d959ea559a5',
			uid: (obj.data.content.fields as { uid: { id: string } }).uid.id,
			balance: Balance.r(SUI.p).new({ value: 0n }),
			option: [5n, null, 3n],
			optionNone: null,
		}),
	);
});

it('calls function correctly when special types are used as as vectors', async () => {
	const txb = new TransactionBlock();

	createSpecialInVectors(txb, 'vector<u64>', {
		string: ['string'],
		asciiString: ['ascii'],
		idField: ['0x0', '0x1'],
		bar: [createBar(txb, 100n)],
		option: [5n, 1n, 3n],
		optionGeneric: [[5n], null],
	});

	const res = await client.signAndExecuteTransactionBlock({
		signer: keypair,
		transactionBlock: txb,
		options: {
			showEffects: true,
		},
	});

	const id = res.effects!.created![0].reference.objectId;

	const obj = await client.getObject({
		id,
		options: {
			showBcs: true,
			showContent: true,
		},
	});

	if (obj.data?.bcs?.dataType !== 'moveObject' || obj.data?.content?.dataType !== 'moveObject') {
		throw new Error(`not a moveObject`);
	}

	expect(WithSpecialTypesInVectors.fromFieldsWithTypes(vector('u64'), obj.data.content)).toEqual(
		WithSpecialTypesInVectors.r(vector('u64')).new({
			id,
			string: ['string'],
			asciiString: ['ascii'],
			idField: [
				'0x0000000000000000000000000000000000000000000000000000000000000000',
				'0x0000000000000000000000000000000000000000000000000000000000000001',
			],
			bar: [Bar.r.new({ value: 100n })],
			option: [5n, 1n, 3n],
			optionGeneric: [[5n], null],
		}),
	);
});

it('loads with loader correctly', async () => {
	const txb = new TransactionBlock();

	const T = `${WithTwoGenerics.$typeName}<${Bar.$typeName}, vector<${WithTwoGenerics.$typeName}<${Bar.$typeName}, u8>>>`;
	const tReified = WithTwoGenerics.reified(
		Bar.reified(),
		vector(WithTwoGenerics.reified(Bar.reified(), 'u8')),
	);

	const withTwoGenerics = createWithTwoGenerics(
		txb,
		[Bar.$typeName, `vector<${WithTwoGenerics.$typeName}<${Bar.$typeName}, u8>>`],
		{
			genericField1: createBar(txb, 100n),
			genericField2: [
				createWithTwoGenerics(txb, [Bar.$typeName, 'u8'], {
					genericField1: createBar(txb, 100n),
					genericField2: 1,
				}),
			],
		},
	);
	createWithGenericField(txb, T, withTwoGenerics);

	const res = await client.signAndExecuteTransactionBlock({
		signer: keypair,
		transactionBlock: txb,
		options: {
			showEffects: true,
		},
	});

	const id = res.effects!.created![0].reference.objectId;

	const obj = await client.getObject({
		id,
		options: {
			showBcs: true,
			showContent: true,
		},
	});
	if (obj.data?.bcs?.dataType !== 'moveObject' || obj.data?.content?.dataType !== 'moveObject') {
		throw new Error(`not a moveObject`);
	}

	initLoaderIfNeeded();

	const withGenericFieldReified = structClassLoaderSource.reified(
		`${WithGenericField.$typeName}<${T}>`,
	);

	expect(extractType(withGenericFieldReified)).toEqual(`${WithGenericField.$typeName}<${T}>`);

	const fromBcs = withGenericFieldReified.fromFieldsWithTypes(obj.data.content);
	expect(fromBcs).toEqual(
		WithGenericField.r(tReified).new({
			id,
			genericField: WithTwoGenerics.r(
				Bar.reified(),
				vector(WithTwoGenerics.reified(Bar.reified(), 'u8')),
			).new({
				genericField1: Bar.r.new({ value: 100n }),
				genericField2: [
					WithTwoGenerics.r(Bar.reified(), 'u8').new({
						genericField1: Bar.r.new({ value: 100n }),
						genericField2: 1,
					}),
				],
			}),
		}),
	);
});

it('converts to json correctly', () => {
	const U = WithSpecialTypes.reified(SUI.p, 'u64');
	const V = vector(WithTwoGenerics.reified(Bar.reified(), 'u8'));

	const obj = WithTwoGenerics.r(U, V).new({
		genericField1: WithSpecialTypes.r(SUI.p, 'u64').new({
			id: '0x1',
			string: 'string',
			asciiString: 'ascii',
			url: 'https://example.com',
			idField: '0x2',
			uid: '0x3',
			balance: Balance.r(SUI.p).new({ value: 0n }),
			option: 100n,
			optionObj: Bar.r.new({ value: 100n }),
			optionNone: null,
			balanceGeneric: Balance.r(SUI.p).new({ value: 0n }),
			optionGeneric: 200n,
			optionGenericNone: null,
		}),
		genericField2: [
			WithTwoGenerics.r(Bar.reified(), 'u8').new({
				genericField1: Bar.r.new({ value: 100n }),
				genericField2: 1,
			}),
		],
	});

	const exp: ReturnType<typeof obj.toJSON> = {
		$typeName:
			'0x8b699fdce543505aeb290ee1b6b5d20fcaa8e8b1a5fc137a8b3facdfa2902209::fixture::WithTwoGenerics',
		$typeArgs: [
			'0x8b699fdce543505aeb290ee1b6b5d20fcaa8e8b1a5fc137a8b3facdfa2902209::fixture::WithSpecialTypes<0x2::sui::SUI, u64>',
			'vector<0x8b699fdce543505aeb290ee1b6b5d20fcaa8e8b1a5fc137a8b3facdfa2902209::fixture::WithTwoGenerics<0x8b699fdce543505aeb290ee1b6b5d20fcaa8e8b1a5fc137a8b3facdfa2902209::fixture::Bar, u8>>',
		],
		genericField1: {
			id: '0x1',
			string: 'string',
			asciiString: 'ascii',
			url: 'https://example.com',
			idField: '0x2',
			uid: '0x3',
			balance: {
				value: '0',
			},
			option: '100',
			optionObj: {
				value: '100',
			},
			optionNone: null,
			balanceGeneric: {
				value: '0',
			},
			optionGeneric: '200',
			optionGenericNone: null,
		},
		genericField2: [
			{
				genericField1: {
					value: '100',
				},
				genericField2: 1,
			},
		],
	};

	const resJSON = obj.toJSON();
	expect(resJSON).toEqual(exp);

	const fromJSON = WithTwoGenerics.fromJSON([U, V], resJSON);
	expect(fromJSON).toEqual(obj);
});

it('decodes address field correctly', async () => {
	const txb = new TransactionBlock();

	const T = 'address';

	const genericVecNested = [
		createWithTwoGenerics(txb, ['address', 'u8'], {
			genericField1: '0x999',
			genericField2: 1,
		}),
	];

	const twoGenericsNested = createWithTwoGenerics(
		txb,
		[T, `${WithTwoGenerics.$typeName}<u8, u8>`],
		{
			genericField1: '0x111',
			genericField2: createWithTwoGenerics(txb, ['u8', 'u8'], {
				genericField1: 1,
				genericField2: 2,
			}),
		},
	);

	const twoGenericsReifiedNested = createWithTwoGenerics(
		txb,
		[Bar.$typeName, `${WithTwoGenerics.$typeName}<u8, u8>`],
		{
			genericField1: createBar(txb, 100n),
			genericField2: createWithTwoGenerics(txb, ['u8', 'u8'], {
				genericField1: 1,
				genericField2: 2,
			}),
		},
	);

	const twoGenericsNestedVec = [
		createWithTwoGenerics(txb, [Bar.$typeName, `vector<${WithTwoGenerics.$typeName}<${T}, u8>>`], {
			genericField1: createBar(txb, 100n),
			genericField2: [
				createWithTwoGenerics(txb, ['address', 'u8'], {
					genericField1: '0x111',
					genericField2: 1,
				}),
			],
		}),
	];

	createFoo(txb, [T, Bar.$typeName], {
		generic: '0x123',
		reifiedPrimitiveVec: [1n, 2n, 3n],
		reifiedObjectVec: [createBar(txb, 100n)],
		genericVec: ['0x555'],
		genericVecNested,
		twoGenerics: createWithTwoGenerics(txb, ['address', Bar.$typeName], {
			genericField1: '0x111',
			genericField2: createBar(txb, 100n),
		}),
		twoGenericsReifiedPrimitive: createWithTwoGenerics(txb, ['u16', 'u64'], {
			genericField1: 1,
			genericField2: 2n,
		}),
		twoGenericsReifiedObject: createWithTwoGenerics(txb, [Bar.$typeName, Bar.$typeName], {
			genericField1: createBar(txb, 100n),
			genericField2: createBar(txb, 100n),
		}),
		twoGenericsNested,
		twoGenericsReifiedNested,
		twoGenericsNestedVec,
		objRef: createBar(txb, 100n),
	});

	const res = await client.signAndExecuteTransactionBlock({
		signer: keypair,
		transactionBlock: txb,
		options: {
			showEffects: true,
		},
	});

	const id = res.effects!.created![0].reference.objectId;

	const foo = await client.getObject({
		id,
		options: {
			showBcs: true,
			showContent: true,
		},
	});

	if (foo.data?.bcs?.dataType !== 'moveObject' || foo.data?.content?.dataType !== 'moveObject') {
		throw new Error(`not a moveObject`);
	}

	const exp = Foo.r('address').new({
		id,
		generic: '0x0000000000000000000000000000000000000000000000000000000000000123',
		reifiedPrimitiveVec: [1n, 2n, 3n],
		reifiedObjectVec: [Bar.r.new({ value: 100n })],
		genericVec: ['0x0000000000000000000000000000000000000000000000000000000000000555'],
		genericVecNested: [
			WithTwoGenerics.r('address', 'u8').new({
				genericField1: '0x0000000000000000000000000000000000000000000000000000000000000999',
				genericField2: 1,
			}),
		],
		twoGenerics: WithTwoGenerics.r('address', Bar.reified()).new({
			genericField1: '0x0000000000000000000000000000000000000000000000000000000000000111',
			genericField2: Bar.r.new({ value: 100n }),
		}),
		twoGenericsReifiedPrimitive: WithTwoGenerics.r('u16', 'u64').new({
			genericField1: 1,
			genericField2: 2n,
		}),
		twoGenericsReifiedObject: WithTwoGenerics.r(Bar.reified(), Bar.reified()).new({
			genericField1: Bar.r.new({ value: 100n }),
			genericField2: Bar.r.new({ value: 100n }),
		}),
		twoGenericsNested: WithTwoGenerics.r('address', WithTwoGenerics.reified('u8', 'u8')).new({
			genericField1: '0x0000000000000000000000000000000000000000000000000000000000000111',
			genericField2: WithTwoGenerics.r('u8', 'u8').new({
				genericField1: 1,
				genericField2: 2,
			}),
		}),
		twoGenericsReifiedNested: WithTwoGenerics.r(
			Bar.reified(),
			WithTwoGenerics.reified('u8', 'u8'),
		).new({
			genericField1: Bar.r.new({ value: 100n }),
			genericField2: WithTwoGenerics.r('u8', 'u8').new({
				genericField1: 1,
				genericField2: 2,
			}),
		}),
		twoGenericsNestedVec: [
			WithTwoGenerics.r(Bar.reified(), vector(WithTwoGenerics.reified('address', 'u8'))).new({
				genericField1: Bar.r.new({ value: 100n }),
				genericField2: [
					WithTwoGenerics.r('address', 'u8').new({
						genericField1: '0x0000000000000000000000000000000000000000000000000000000000000111',
						genericField2: 1,
					}),
				],
			}),
		],
		dummy: Dummy.r.new({ dummyField: false }),
		other: StructFromOtherModule.r.new({ dummyField: false }),
	});

	expect(Foo.fromBcs('address', fromB64(foo.data.bcs.bcsBytes))).toEqual(exp);
	expect(Foo.fromFieldsWithTypes('address', foo.data.content)).toEqual(exp);
	expect(Foo.fromSuiParsedData('address', foo.data.content)).toEqual(exp);
	expect(await Foo.fetch(client, 'address', id)).toEqual(exp);

	const de = Foo.fromFieldsWithTypes('address', foo.data.content);
	expect(Foo.fromJSON('address', de.toJSON())).toEqual(exp);
});
