// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { execSync } from 'child_process';
import path from 'path';
import type { SuiObjectChangePublished } from '@mysten/sui/client';
import { getFullnodeUrl, SuiClient } from '@mysten/sui/client';
import { FaucetRateLimitError, getFaucetHost, requestSuiFromFaucetV0 } from '@mysten/sui/faucet';
import { Ed25519Keypair } from '@mysten/sui/keypairs/ed25519';
import { Transaction } from '@mysten/sui/transactions';
import { retry } from 'ts-retry-promise';
import { expect } from 'vitest';

import type { CoinMap } from '../src/utils/constants.js';

const DEFAULT_FAUCET_URL = process.env.VITE_FAUCET_URL ?? getFaucetHost('localnet');
const DEFAULT_FULLNODE_URL = process.env.VITE_FULLNODE_URL ?? getFullnodeUrl('localnet');
const SUI_BIN = process.env.VITE_SUI_BIN ?? path.resolve(process.cwd(), '../../target/debug/sui');

export const DEFAULT_TICK_SIZE = 1n;
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

// TODO: expose these testing utils from @mysten/sui
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

export async function publishCoins(toolbox?: TestToolbox): Promise<CoinMap> {
	if (!toolbox) {
		toolbox = await setupSuiClient();
	}
	const tokenSourcesPath = path.join(__dirname, 'data/token');
	writeToml(tokenSourcesPath, '0x0', 'token');
	let deepRes = await publishPackage(tokenSourcesPath, toolbox);
	writeToml(tokenSourcesPath, deepRes.packageId, 'token');

	const usdcSourcePath = path.join(__dirname, 'data/usdc');
	const usdcRes = await publishPackage(usdcSourcePath, toolbox);

	const spamSourcePath = path.join(__dirname, 'data/spam');
	const spamRes = await publishPackage(spamSourcePath, toolbox);

	return {
		DEEP: {
			address: deepRes.packageId,
			type: `${deepRes.packageId}::deep::DEEP`,
			scalar: 1000000,
		},
		USDC: {
			address: usdcRes.packageId,
			type: `${usdcRes.packageId}::usdc::USDC`,
			scalar: 1000000,
		},
		SPAM: {
			address: spamRes.packageId,
			type: `${spamRes.packageId}::spam::SPAM`,
			scalar: 1000000,
		},
	};
}

export async function publishDeepBook(toolbox?: TestToolbox) {
	if (!toolbox) {
		toolbox = await setupSuiClient();
	}

	const deepbookSourcesPath = path.join(__dirname, 'data/deepbook');
	let deepbookRes = await publishPackage(deepbookSourcesPath, toolbox);

	const deepbookPackageId = deepbookRes.packageId;
	// @ts-ignore
	const deepbookRegistryId = deepbookRes.publishTxn.objectChanges?.find((change) => {
		return (
			change.type === 'created' &&
			change.objectType.includes('Registry') &&
			!change.objectType.includes('Inner')
		);
	})?.['objectId'];

	// @ts-ignore
	const deepbookAdminCap = deepbookRes.publishTxn.objectChanges?.find((change) => {
		return change.type === 'created' && change.objectType.includes('DeepbookAdminCap');
	})?.['objectId'];

	return { deepbookPackageId, deepbookRegistryId, deepbookAdminCap };
}

// TODO: expose these testing utils from @mysten/sui
export async function publishPackage(packagePath: string, toolbox?: TestToolbox) {
	// TODO: We create a unique publish address per publish, but we really could share one for all publishes.
	if (!toolbox) {
		toolbox = await setupSuiClient();
	}

	const { modules, dependencies } = JSON.parse(
		execSync(`${SUI_BIN} move build --dump-bytecode-as-base64 --path ${packagePath}`, {
			encoding: 'utf-8',
		}),
	);
	const tx = new Transaction();
	const cap = tx.publish({
		modules,
		dependencies,
	});

	// Transfer the upgrade capability to the sender so they can upgrade the package later if they want.
	tx.transferObjects([cap], toolbox.address());

	const { digest } = await toolbox.client.signAndExecuteTransaction({
		transaction: tx,
		signer: toolbox.keypair,
	});

	const publishTxn = await toolbox.client.waitForTransaction({
		digest: digest,
		options: { showObjectChanges: true, showEffects: true },
	});

	expect(publishTxn.effects?.status.status).toEqual('success');

	const packageId = ((publishTxn.objectChanges?.filter(
		(a) => a.type === 'published',
	) as SuiObjectChangePublished[]) ?? [])[0]?.packageId.replace(/^(0x)(0+)/, '0x') as string;

	expect(packageId).toBeTypeOf('string');

	console.info(`Published package ${packageId} from address ${toolbox.address()}}`);

	return { packageId, publishTxn };
}
