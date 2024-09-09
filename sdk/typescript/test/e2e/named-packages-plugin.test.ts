// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from 'vitest';

import { SuiGraphQLClient } from '../../src/graphql/client';
import { namedPackagesPlugin, Transaction } from '../../src/transactions';
import { normalizeSuiAddress } from '../../src/utils';

Transaction.registerGlobalSerializationPlugin(
	'namedPackagesPlugin',
	namedPackagesPlugin({
		suiGraphQLClient: new SuiGraphQLClient({
			url: 'http://127.0.0.1:9125',
		}),
		overrides: {
			packages: {
				'std@framework': '0x1',
				'std@framework/v1': '0x1',
			},
			types: {
				'std@framework::string::String': '0x1::string::String',
				'std@framework::vector::empty<std@framework::string::String>':
					'0x1::vector::empty<0x1::string::String>',
			},
		},
	}),
);

describe('Name Resolution Plugin (.move)', () => {
	it('Should replace names in a given PTB', async () => {
		const transaction = new Transaction();

		// replace .move names properly
		transaction.moveCall({
			target: 'std@framework::string::utf8',
			arguments: [transaction.pure.string('Hello, world!')],
		});

		// replace type args properly
		transaction.moveCall({
			target: 'std@framework::vector::empty',
			typeArguments: ['std@framework::string::String'],
		});

		// replace nested type args properly
		transaction.moveCall({
			target: 'std@framework/v1::vector::empty',
			typeArguments: ['std@framework::vector::empty<std@framework::string::String>'],
		});

		const json = JSON.parse(await transaction.toJSON());

		expect(json.commands[0].MoveCall.package).toBe(normalizeSuiAddress('0x1'));
		expect(json.commands[1].MoveCall.typeArguments[0]).toBe(`0x1::string::String`);
		expect(json.commands[2].MoveCall.package).toBe(normalizeSuiAddress('0x1'));
		expect(json.commands[2].MoveCall.typeArguments[0]).toBe(
			`0x1::vector::empty<0x1::string::String>`,
		);
	});

	// TODO: Add some tests utilizing live GraphQL Queries (mainnet / testnet),
	// not just overrides.
});
