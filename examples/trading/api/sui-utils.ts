// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { execSync } from 'child_process';
import { readFileSync, writeFileSync } from 'fs';
import { homedir } from 'os';
import path from 'path';
import { getFullnodeUrl, SuiClient } from '@mysten/sui.js/client';
import { Ed25519Keypair } from '@mysten/sui.js/keypairs/ed25519';
import { TransactionBlock } from '@mysten/sui.js/transactions';
import { fromB64 } from '@mysten/sui.js/utils';

export type Network = 'mainnet' | 'testnet' | 'devnet' | 'localnet';

export const ACTIVE_NETWORK = (process.env.NETWORK as Network) || 'testnet';

export const SUI_BIN = `sui`;

export const getActiveAddress = () => {
	return execSync(`${SUI_BIN} client active-address`, { encoding: 'utf8' }).trim();
};

/** Returns a signer based on the active address of system's sui. */
export const getSigner = () => {
	const sender = getActiveAddress();

	const keystore = JSON.parse(
		readFileSync(path.join(homedir(), '.sui', 'sui_config', 'sui.keystore'), 'utf8'),
	);

	for (const priv of keystore) {
		const raw = fromB64(priv);
		if (raw[0] !== 0) {
			continue;
		}

		const pair = Ed25519Keypair.fromSecretKey(raw.slice(1));
		if (pair.getPublicKey().toSuiAddress() === sender) {
			return pair;
		}
	}

	throw new Error(`keypair not found for sender: ${sender}`);
};

/** Get the client for the specified network. */
export const getClient = (network: Network) => {
	return new SuiClient({ url: getFullnodeUrl(network) });
};

/** A helper to sign & execute a transaction. */
export const signAndExecute = async (txb: TransactionBlock, network: Network) => {
	const client = getClient(network);
	const signer = getSigner();

	return client.signAndExecuteTransactionBlock({
		transactionBlock: txb,
		signer,
		options: {
			showEffects: true,
			showObjectChanges: true,
		},
	});
};

/** Publishes a package and saves the package id to a specified json file. */
export const publishPackage = async ({
	packagePath,
	network,
	exportFileName = 'contract',
}: {
	packagePath: string;
	network: Network;
	exportFileName: string;
}) => {
	const txb = new TransactionBlock();

	const { modules, dependencies } = JSON.parse(
		execSync(`${SUI_BIN} move build --dump-bytecode-as-base64 --path ${packagePath}`, {
			encoding: 'utf-8',
		}),
	);

	const cap = txb.publish({
		modules,
		dependencies,
	});

	// Transfer the upgrade capability to the sender so they can upgrade the package later if they want.
	txb.transferObjects([cap], txb.pure(getActiveAddress()));

	const results = await signAndExecute(txb, network);

	// @ts-ignore-next-line
	const packageId = results.objectChanges?.find((x) => x.type === 'published')?.packageId;

	// save to an env file
	writeFileSync(
		`${exportFileName}.json`,
		JSON.stringify({
			packageId,
		}),
		{ encoding: 'utf8', flag: 'w' },
	);
};
