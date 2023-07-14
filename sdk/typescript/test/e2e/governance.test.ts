// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeAll } from 'vitest';
import { getExecutionStatusType, SuiSystemStateUtil, SUI_TYPE_ARG } from '../../src';
import { setup, TestToolbox } from './utils/setup';
import { Keypair } from '../../src/cryptography';
import { SuiClient } from '../../src/client';

const DEFAULT_STAKE_AMOUNT = 1000000000;

describe('Governance API', () => {
	let toolbox: TestToolbox;

	beforeAll(async () => {
		toolbox = await setup();
	});

	it('test requestAddStake', async () => {
		const result = await addStake(toolbox.client, toolbox.keypair);
		expect(getExecutionStatusType(result)).toEqual('success');
	});

	it('test getDelegatedStakes', async () => {
		await addStake(toolbox.client, toolbox.keypair);
		const stakes = await toolbox.client.getStakes({
			owner: toolbox.address(),
		});
		const stakesById = await toolbox.client.getStakesByIds({
			stakedSuiIds: [stakes[0].stakes[0].stakedSuiId],
		});
		expect(stakes.length).greaterThan(0);
		expect(stakesById[0].stakes[0]).toEqual(stakes[0].stakes[0]);
	});

	it('test requestWithdrawStake', async () => {
		// TODO: implement this
	});

	it('test getCommitteeInfo', async () => {
		const committeeInfo = await toolbox.client.getCommitteeInfo({
			epoch: '0',
		});
		expect(committeeInfo.validators?.length).greaterThan(0);
	});

	it('test getLatestSuiSystemState', async () => {
		await toolbox.client.getLatestSuiSystemState();
	});
});

async function addStake(client: SuiClient, signer: Keypair) {
	const coins = await client.getCoins({
		owner: await signer.getPublicKey().toSuiAddress(),
		coinType: SUI_TYPE_ARG,
	});

	const system = await client.getLatestSuiSystemState();
	const validators = system.activeValidators;

	const tx = await SuiSystemStateUtil.newRequestAddStakeTxn(
		client,
		[coins.data[0].coinObjectId],
		BigInt(DEFAULT_STAKE_AMOUNT),
		validators[0].suiAddress,
	);

	return await client.signAndExecuteTransactionBlock({
		signer,
		transactionBlock: tx,
		options: {
			showEffects: true,
		},
	});
}
