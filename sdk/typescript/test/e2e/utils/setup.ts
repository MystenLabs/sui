// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { expect } from 'vitest';
import { execSync } from 'child_process';
import tmp from 'tmp';

import {
	getPublishedObjectChanges,
	getExecutionStatusType,
	Coin,
	UpgradePolicy,
} from '../../../src';
import { TransactionBlock } from '../../../src/builder';
import { Ed25519Keypair } from '../../../src/keypairs/ed25519';
import { retry } from 'ts-retry-promise';
import { FaucetRateLimitError, getFaucetHost, requestSuiFromFaucetV0 } from '../../../src/faucet';
import { SuiClient, getFullnodeUrl } from '../../../src/client';
import { Keypair } from '../../../src/cryptography';

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

	// TODO(chris): replace this with provider.getCoins instead
	async getGasObjectsOwnedByAddress() {
		const objects = await this.client.getOwnedObjects({
			owner: this.address(),
			options: {
				showType: true,
				showContent: true,
				showOwner: true,
			},
		});
		return objects.data.filter((obj) => Coin.isSUI(obj));
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

export async function setup() {
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
	expect(getExecutionStatusType(publishTxn)).toEqual('success');

	const packageId = getPublishedObjectChanges(publishTxn)[0].packageId.replace(
		/^(0x)(0+)/,
		'0x',
	) as string;

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

	expect(getExecutionStatusType(result)).toEqual('success');
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
		const coin = tx.splitCoins(tx.object(coinId!), [tx.pure(amounts![i])]);
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
	expect(getExecutionStatusType(txn)).toEqual('success');
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
