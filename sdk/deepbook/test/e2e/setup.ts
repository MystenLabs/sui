// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { execSync } from 'child_process';
import type {
	DevInspectResults,
	SuiObjectChangeCreated,
	SuiObjectChangePublished,
	SuiTransactionBlockResponse,
} from '@mysten/sui.js/client';
import { getFullnodeUrl, SuiClient } from '@mysten/sui.js/client';
import { FaucetRateLimitError, getFaucetHost, requestSuiFromFaucetV0 } from '@mysten/sui.js/faucet';
import { Ed25519Keypair } from '@mysten/sui.js/keypairs/ed25519';
import { TransactionBlock } from '@mysten/sui.js/transactions';
import tmp from 'tmp';
import { retry } from 'ts-retry-promise';
import { expect } from 'vitest';

import { DeepBookClient } from '../../src/index.js';
import type { PoolSummary } from '../../src/types/index.js';
import { FLOAT_SCALING_FACTOR, NORMALIZED_SUI_COIN_TYPE } from '../../src/utils/index.js';

const DEFAULT_FAUCET_URL = import.meta.env.VITE_FAUCET_URL ?? getFaucetHost('localnet');
const DEFAULT_FULLNODE_URL = import.meta.env.VITE_FULLNODE_URL ?? getFullnodeUrl('localnet');
const SUI_BIN = import.meta.env.VITE_SUI_BIN ?? 'cargo run --bin sui';

export const DEFAULT_TICK_SIZE = 1n * FLOAT_SCALING_FACTOR;
export const DEFAULT_LOT_SIZE = 1n;

export class TestToolbox {
	keypair: Ed25519Keypair;
	client: SuiClient;

	constructor(keypair: Ed25519Keypair, client: SuiClient) {
		this.keypair = keypair;
		this.client = client;
	}

	address() {
		return this.keypair.getPublicKey().toSuiAddress();
	}

	public async getActiveValidators() {
		return (await this.client.getLatestSuiSystemState()).activeValidators;
	}
}

export function getClient(): SuiClient {
	return new SuiClient({
		url: DEFAULT_FULLNODE_URL,
	});
}

// TODO: expose these testing utils from @mysten/sui.js
export async function setupSuiClient() {
	const keypair = Ed25519Keypair.generate();
	const address = keypair.getPublicKey().toSuiAddress();
	const client = getClient();
	await retry(() => requestSuiFromFaucetV0({ host: DEFAULT_FAUCET_URL, recipient: address }), {
		backoff: 'EXPONENTIAL',
		// overall timeout in 60 seconds
		timeout: 1000 * 60,
		// skip retry if we hit the rate-limit error
		retryIf: (error: any) => !(error instanceof FaucetRateLimitError),
		logger: (msg) => console.warn('Retrying requesting from faucet: ' + msg),
	});
	return new TestToolbox(keypair, client);
}

// TODO: expose these testing utils from @mysten/sui.js
export async function publishPackage(packagePath: string, toolbox?: TestToolbox) {
	// TODO: We create a unique publish address per publish, but we really could share one for all publishes.
	if (!toolbox) {
		toolbox = await setupSuiClient();
	}

	// remove all controlled temporary objects on process exit
	tmp.setGracefulCleanup();

	const tmpobj = tmp.dirSync({ unsafeCleanup: true });

	const { modules, dependencies } = JSON.parse(
		execSync(
			`${SUI_BIN} move build --dump-bytecode-as-base64 --path ${packagePath} --install-dir ${tmpobj.name}`,
			{ encoding: 'utf-8' },
		),
	);
	const tx = new TransactionBlock();
	const cap = tx.publish({
		modules,
		dependencies,
	});

	// Transfer the upgrade capability to the sender so they can upgrade the package later if they want.
	tx.transferObjects([cap], tx.pure(await toolbox.address()));

	const publishTxn = await toolbox.client.signAndExecuteTransactionBlock({
		transactionBlock: tx,
		signer: toolbox.keypair,
		options: {
			showEffects: true,
			showObjectChanges: true,
		},
	});
	expect(publishTxn.effects?.status.status).toEqual('success');

	const packageId = ((publishTxn.objectChanges?.filter(
		(a) => a.type === 'published',
	) as SuiObjectChangePublished[]) ?? [])[0].packageId.replace(/^(0x)(0+)/, '0x') as string;

	expect(packageId).toBeTypeOf('string');

	console.info(`Published package ${packageId} from address ${toolbox.address()}}`);

	return { packageId, publishTxn };
}

export async function setupPool(toolbox: TestToolbox): Promise<PoolSummary> {
	const packagePath = __dirname + '/./data/test_coin';
	const { packageId } = await publishPackage(packagePath, toolbox);
	const baseAsset = `${packageId}::test::TEST`;
	const quoteAsset = NORMALIZED_SUI_COIN_TYPE;
	const deepbook = new DeepBookClient(toolbox.client);
	const txb = deepbook.createPool(baseAsset, quoteAsset, DEFAULT_TICK_SIZE, DEFAULT_LOT_SIZE);
	const resp = await executeTransactionBlock(toolbox, txb);
	const event = resp.events?.find((e) => e.type.includes('PoolCreated')) as any;
	return {
		poolId: event.parsedJson.pool_id,
		baseAsset,
		quoteAsset,
	};
}

export async function setupDeepbookAccount(toolbox: TestToolbox): Promise<string> {
	const deepbook = new DeepBookClient(toolbox.client);
	const txb = deepbook.createAccount(toolbox.address());
	const resp = await executeTransactionBlock(toolbox, txb);

	const accountCap = ((resp.objectChanges?.filter(
		(a) => a.type === 'created',
	) as SuiObjectChangeCreated[]) ?? [])[0].objectId;
	return accountCap;
}

export async function executeTransactionBlock(
	toolbox: TestToolbox,
	txb: TransactionBlock,
): Promise<SuiTransactionBlockResponse> {
	const resp = await toolbox.client.signAndExecuteTransactionBlock({
		signer: toolbox.keypair,
		transactionBlock: txb,
		options: {
			showEffects: true,
			showEvents: true,
			showObjectChanges: true,
		},
	});
	expect(resp.effects?.status.status).toEqual('success');
	return resp;
}

export async function devInspectTransactionBlock(
	toolbox: TestToolbox,
	txb: TransactionBlock,
): Promise<DevInspectResults> {
	return await toolbox.client.devInspectTransactionBlock({
		transactionBlock: txb,
		sender: toolbox.address(),
	});
}
