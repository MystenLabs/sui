// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeAll } from 'vitest';

import { setup, TestToolbox } from './utils/setup';

const DEFAULT_PACKAGE = '0x2';
const DEFAULT_MODULE = 'coin';
const DEFAULT_FUNCTION = 'balance';
const DEFAULT_STRUCT = 'Coin';

describe('Normalized modules API', () => {
	let toolbox: TestToolbox;

	beforeAll(async () => {
		toolbox = await setup();
	});

	it('Get Move function arg types', async () => {
		const argTypes = await toolbox.client.getMoveFunctionArgTypes({
			package: DEFAULT_PACKAGE,
			module: DEFAULT_MODULE,
			function: DEFAULT_FUNCTION,
		});
		expect(argTypes).toEqual([
			{
				Object: 'ByImmutableReference',
			},
		]);
	});

	it('Get Normalized Modules by packages', async () => {
		const modules = await toolbox.client.getNormalizedMoveModulesByPackage({
			package: DEFAULT_PACKAGE,
		});
		expect(Object.keys(modules)).contains(DEFAULT_MODULE);
	});

	it('Get Normalized Move Module', async () => {
		const normalized = await toolbox.client.getNormalizedMoveModule({
			package: DEFAULT_PACKAGE,
			module: DEFAULT_MODULE,
		});
		expect(Object.keys(normalized.exposedFunctions)).toContain(DEFAULT_FUNCTION);
	});

	it('Get Normalized Move Function', async () => {
		const normalized = await toolbox.client.getNormalizedMoveFunction({
			package: DEFAULT_PACKAGE,
			module: DEFAULT_MODULE,
			function: DEFAULT_FUNCTION,
		});
		expect(normalized.isEntry).toEqual(false);
	});

	it('Get Normalized Move Struct ', async () => {
		const struct = await toolbox.client.getNormalizedMoveStruct({
			package: DEFAULT_PACKAGE,
			module: DEFAULT_MODULE,
			struct: DEFAULT_STRUCT,
		});
		expect(struct.fields.length).toBeGreaterThan(1);
	});
});
