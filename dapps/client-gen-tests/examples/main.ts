// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { BCS } from '@mysten/bcs';
import { SuiClient } from '@mysten/sui.js/client';
import { Ed25519Keypair } from '@mysten/sui.js/keypairs/ed25519';
import { TransactionBlock } from '@mysten/sui.js/transactions';
import { fromB64, normalizeSuiAddress } from '@mysten/sui.js/utils';
import { Command } from 'commander';

import { vector } from './gen/_framework/reified';
import { Pool, PoolCreationEvent, PoolRegistry, PoolRegistryItem } from './gen/amm/pool/structs';
import { createPoolWithCoins } from './gen/amm/util/functions';
import { PACKAGE_ID as EXAMPLES_PACKAGE_ID } from './gen/examples';
import { faucetMint } from './gen/examples/example-coin/functions';
import { EXAMPLE_COIN } from './gen/examples/example-coin/structs';
import { createExampleStruct, specialTypes } from './gen/examples/examples/functions';
import { ExampleStruct } from './gen/examples/examples/structs';
import { createWithGenericField } from './gen/examples/fixture/functions';
import { WithGenericField } from './gen/examples/fixture/structs';
import { Field } from './gen/sui/dynamic-field/structs';
import { SUI } from './gen/sui/sui/structs';

const EXAMPLE_COIN_FAUCET_ID = '0x23a00d64a785280a794d0bdd2f641dfabf117c78e07cb682550ed3c2b41dd760';
const AMM_POOL_REGISTRY_ID = '0xe3e05313eff4f6f44206982e42fa1219c972113f3a651abe168123abc0202411';

const AMM_POOL_ID = '0x799331284a2f75ed54b1a2bf212a26e3f465cbc7b974dbfa956f093de9ad8059';

const WITH_GENERIC_FIELD_ID = '0xf170bc37f72659e942b376cef95b3194f8ffbecc0a82e601d682ae6e2693cd35';

const keypair = Ed25519Keypair.fromSecretKey(
	fromB64('AMVT58FaLF2tJtg/g8X2z1/vG0FvNn0jvRu9X2Wl8F+u').slice(1),
); // address: 0x8becfafb14c111fc08adee6cc9afa95a863d1bf133f796626eec353f98ea8507

const client = new SuiClient({
	url: 'https://fullnode.testnet.sui.io:443/',
});

/**
 * An example for calling transactions.
 * Create a new AMM pool. Will not work if the pool already exists.
 */
async function createPool() {
	const address = keypair.getPublicKey().toSuiAddress();

	const txb = new TransactionBlock();

	const [suiCoin] = txb.splitCoins(txb.gas, [txb.pure(1_000_000)]);
	const exampleCoin = faucetMint(txb, EXAMPLE_COIN_FAUCET_ID);
	const lp = createPoolWithCoins(txb, ['0x2::sui::SUI', EXAMPLE_COIN.$typeName], {
		registry: AMM_POOL_REGISTRY_ID,
		initA: suiCoin,
		initB: exampleCoin,
		lpFeeBps: 30n,
		adminFeePct: 10n,
	});
	txb.transferObjects([lp], txb.pure(address));

	const res = await client.signAndExecuteTransactionBlock({
		signer: keypair,
		transactionBlock: txb,
	});
	console.log(`tx digest: ${res.digest}`);
}

/** An example for object fetching. Fetch and print the AMM pool at AMM_POOL_ID. */
async function fetchPool() {
	const pool = await Pool.r(SUI.p, EXAMPLE_COIN.p).fetch(client, AMM_POOL_ID);
	console.log(pool);
}

/** An example for event fetching. Fetch and print the pool creation events. */
async function fetchPoolCreationEvents() {
	const res = await client.queryEvents({
		query: {
			MoveEventType: PoolCreationEvent.$typeName,
		},
	});
	res.data.forEach((e) => {
		console.log(PoolCreationEvent.fromBcs(fromB64(e.bcs!)));
	});
}

/**
 * An example for dynamic field fetching.
 * Fetch and print the items in the AMM pool registry at AMM_POOL_REGISTRY_ID.
 */
async function fetchPoolRegistryItems() {
	const registry = await PoolRegistry.fetch(client, AMM_POOL_REGISTRY_ID);
	const fields = await client.getDynamicFields({
		parentId: registry.table.id,
	});

	const item = await Field.fetch(
		client,
		[PoolRegistryItem.reified(), 'bool'],
		fields.data[0].objectId,
	);
	console.log(item);
}

/**
 * An example for calling transactions with generic fields.
 */
async function createStructWithVector() {
	const txb = new TransactionBlock();

	const coin = faucetMint(txb, txb.object(EXAMPLE_COIN_FAUCET_ID));

	const field = txb.makeMoveVec({
		objects: [coin],
	});
	createWithGenericField(
		txb,
		`vector<0x2::coin::Coin<${EXAMPLES_PACKAGE_ID}::example_coin::EXAMPLE_COIN>>`,
		field,
	);

	const res = await client.signAndExecuteTransactionBlock({
		signer: keypair,
		transactionBlock: txb,
	});
	console.log(res);
}

/** An example for object fetching with generic fields. */
async function fetchWithGenericField() {
	const field = await WithGenericField.r(vector(EXAMPLE_COIN.r)).fetch(
		client,
		WITH_GENERIC_FIELD_ID,
	);
	console.log(field);
}

async function createSpecialTypes() {
	const txb = new TransactionBlock();

	const e1 = createExampleStruct(txb);
	const e2 = createExampleStruct(txb);

	specialTypes(txb, {
		asciiString: 'example ascii string',
		utf8String: 'example utf8 string',
		vectorOfU64: [1n, 2n],
		vectorOfObjects: [e1, e2],
		idField: '0x12345',
		address: '0x12345',
		optionSome: 5n,
		optionNone: null,
	});

	// manually
	specialTypes(txb, {
		asciiString: txb.pure('example ascii string', BCS.STRING),
		utf8String: txb.pure('example utf8 string', BCS.STRING),
		vectorOfU64: txb.pure([1n, 2n], 'vector<u64>'),
		vectorOfObjects: txb.makeMoveVec({
			objects: [createExampleStruct(txb), createExampleStruct(txb)],
			type: ExampleStruct.$typeName,
		}),
		idField: txb.pure(normalizeSuiAddress('0x12345'), BCS.ADDRESS),
		address: txb.pure(normalizeSuiAddress('0x12345'), BCS.ADDRESS),
		optionSome: txb.pure([5n], 'vector<u64>'),
		optionNone: txb.pure([], 'vector<u64>'),
	});

	const res = await client.signAndExecuteTransactionBlock({
		signer: keypair,
		transactionBlock: txb,
	});
	console.log(res.digest);
}

async function main() {
	const program = new Command();

	program
		.command('create-pool')
		.action(createPool)
		.summary(
			'An example for calling transactions. Create a new AMM pool. Will not work if the pool already exists.',
		);
	program
		.command('fetch-pool')
		.action(fetchPool)
		.summary(`An example for object fetching. Fetch and print the AMM pool at ${AMM_POOL_ID}.`);
	program
		.command('fetch-pool-registry-items')
		.action(fetchPoolRegistryItems)
		.summary(
			`An example for dynamic field fetching. Fetch and print the items in the AMM pool registry at ${AMM_POOL_REGISTRY_ID}.`,
		);
	program
		.command('fetch-pool-creation-events')
		.action(fetchPoolCreationEvents)
		.summary('An example for event fetching. Fetch and print the pool creation events.');
	program
		.command('create-struct-with-vector')
		.action(createStructWithVector)
		.summary('An example for calling transactions with generic fields.');
	program
		.command('fetch-with-generic-field')
		.action(fetchWithGenericField)
		.summary('An example for fetching an object with a generic field.');
	program
		.command('create-special-types')
		.action(createSpecialTypes)
		.summary('An example for calling functions with special types.');

	program.addHelpCommand(false);

	await program.parseAsync();
}

main().catch(console.error);
