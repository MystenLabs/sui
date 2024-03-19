// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { execSync } from 'child_process';
import tmp from 'tmp';
import { retry } from 'ts-retry-promise';
import { expect } from 'vitest';
import { WebSocket } from 'ws';

import type { SuiObjectChangePublished } from '../../../src/client/index.js';
import { getFullnodeUrl, SuiClient, SuiHTTPTransport } from '../../../src/client/index.js';
import type { Keypair } from '../../../src/cryptography/index.js';
import {
	FaucetRateLimitError,
	getFaucetHost,
	requestSuiFromFaucetV0,
} from '../../../src/faucet/index.js';
import { Ed25519Keypair } from '../../../src/keypairs/ed25519/index.js';
import { TransactionBlock, UpgradePolicy } from '../../../src/transactions/index.js';
import { SUI_TYPE_ARG } from '../../../src/utils/index.js';

const DEFAULT_FAUCET_URL = import.meta.env.VITE_FAUCET_URL ?? getFaucetHost('localnet');
const DEFAULT_FULLNODE_URL = import.meta.env.VITE_FULLNODE_URL ?? getFullnodeUrl('localnet');

const SUI_BIN = import.meta.env.VITE_SUI_BIN ?? 'cargo run --bin sui';

export const DEFAULT_RECIPIENT =
	'0x0c567ffdf8162cb6d51af74be0199443b92e823d4ba6ced24de5c6c463797d46';
export const DEFAULT_RECIPIENT_2 =
	'0xbb967ddbebfee8c40d8fdd2c24cb02452834cd3a7061d18564448f900eb9e66d';
export const DEFAULT_GAS_BUDGET = 10000000;
export const DEFAULT_SEND_AMOUNT = 1000;

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

	async getGasObjectsOwnedByAddress() {
		return await this.client.getCoins({
			owner: this.address(),
			coinType: SUI_TYPE_ARG,
		});
	}

	public async getActiveValidators() {
		return (await this.client.getLatestSuiSystemState()).activeValidators;
	}
}

export function getClient(url = DEFAULT_FULLNODE_URL): SuiClient {
	return new SuiClient({
		transport: new SuiHTTPTransport({
			url,
			WebSocketConstructor: WebSocket as never,
		}),
	});
}

export async function setup(options: { graphQLURL?: string; rpcURL?: string } = {}) {
	const keypair = Ed25519Keypair.generate();
	const address = keypair.getPublicKey().toSuiAddress();
	return setupWithFundedAddress(keypair, address, options);
}

export async function setupWithFundedAddress(
	keypair: Ed25519Keypair,
	address: string,
	{ rpcURL }: { graphQLURL?: string; rpcURL?: string } = {},
) {
	const client = getClient(rpcURL);
	await retry(() => requestSuiFromFaucetV0({ host: DEFAULT_FAUCET_URL, recipient: address }), {
		backoff: 'EXPONENTIAL',
		// overall timeout in 60 seconds
		timeout: 1000 * 60,
		// skip retry if we hit the rate-limit error
		retryIf: (error: any) => !(error instanceof FaucetRateLimitError),
		logger: (msg) => console.warn('Retrying requesting from faucet: ' + msg),
	});

	await retry(
		async () => {
			const balance = await client.getBalance({ owner: address });

			if (balance.totalBalance === '0') {
				throw new Error('Balance is still 0');
			}
		},
		{
			backoff: () => 1000,
			timeout: 30 * 1000,
			retryIf: () => true,
		},
	);
	return new TestToolbox(keypair, client);
}

export async function publishPackage(packagePath: string, toolbox?: TestToolbox) {
	// TODO: We create a unique publish address per publish, but we really could share one for all publishes.
	if (!toolbox) {
		toolbox = await setup();
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

	await toolbox.client.waitForTransactionBlock({ digest: publishTxn.digest });

	expect(publishTxn.effects?.status.status).toEqual('success');

	const packageId = ((publishTxn.objectChanges?.filter(
		(a) => a.type === 'published',
	) as SuiObjectChangePublished[]) ?? [])[0].packageId.replace(/^(0x)(0+)/, '0x') as string;

	expect(packageId).toBeTypeOf('string');

	console.info(`Published package ${packageId} from address ${toolbox.address()}}`);

	return { packageId, publishTxn };
}

export async function upgradePackage(
	packageId: string,
	capId: string,
	packagePath: string,
	toolbox?: TestToolbox,
) {
	// TODO: We create a unique publish address per publish, but we really could share one for all publishes.
	if (!toolbox) {
		toolbox = await setup();
	}

	// remove all controlled temporary objects on process exit
	tmp.setGracefulCleanup();

	const tmpobj = tmp.dirSync({ unsafeCleanup: true });

	const { modules, dependencies, digest } = JSON.parse(
		execSync(
			`${SUI_BIN} move build --dump-bytecode-as-base64 --path ${packagePath} --install-dir ${tmpobj.name}`,
			{ encoding: 'utf-8' },
		),
	);

	const tx = new TransactionBlock();

	const cap = tx.object(capId);
	const ticket = tx.moveCall({
		target: '0x2::package::authorize_upgrade',
		arguments: [cap, tx.pure(UpgradePolicy.COMPATIBLE), tx.pure(digest)],
	});

	const receipt = tx.upgrade({
		modules,
		dependencies,
		packageId,
		ticket,
	});

	tx.moveCall({
		target: '0x2::package::commit_upgrade',
		arguments: [cap, receipt],
	});

	const result = await toolbox.client.signAndExecuteTransactionBlock({
		transactionBlock: tx,
		signer: toolbox.keypair,
		options: {
			showEffects: true,
			showObjectChanges: true,
		},
	});

	expect(result.effects?.status.status).toEqual('success');
}

export function getRandomAddresses(n: number): string[] {
	return Array(n)
		.fill(null)
		.map(() => {
			const keypair = Ed25519Keypair.generate();
			return keypair.getPublicKey().toSuiAddress();
		});
}

export async function paySui(
	client: SuiClient,
	signer: Keypair,
	numRecipients: number = 1,
	recipients?: string[],
	amounts?: number[],
	coinId?: string,
) {
	const tx = new TransactionBlock();

	recipients = recipients ?? getRandomAddresses(numRecipients);
	amounts = amounts ?? Array(numRecipients).fill(DEFAULT_SEND_AMOUNT);

	expect(recipients.length === amounts.length, 'recipients and amounts must be the same length');

	coinId =
		coinId ??
		(
			await client.getCoins({
				owner: signer.getPublicKey().toSuiAddress(),
				coinType: '0x2::sui::SUI',
			})
		).data[0].coinObjectId;

	recipients.forEach((recipient, i) => {
		const coin = tx.splitCoins(coinId!, [tx.pure(amounts![i])]);
		tx.transferObjects([coin], tx.pure(recipient));
	});

	const txn = await client.signAndExecuteTransactionBlock({
		transactionBlock: tx,
		signer,
		options: {
			showEffects: true,
			showObjectChanges: true,
		},
	});
	expect(txn.effects?.status.status).toEqual('success');
	return txn;
}

export async function executePaySuiNTimes(
	client: SuiClient,
	signer: Keypair,
	nTimes: number,
	numRecipientsPerTxn: number = 1,
	recipients?: string[],
	amounts?: number[],
) {
	const txns = [];
	for (let i = 0; i < nTimes; i++) {
		// must await here to make sure the txns are executed in order
		txns.push(await paySui(client, signer, numRecipientsPerTxn, recipients, amounts));
	}
	return txns;
}
