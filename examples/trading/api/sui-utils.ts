// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { execSync } from 'child_process';
import { writeFileSync } from 'fs';
import { getFullnodeUrl, SuiClient } from '@mysten/sui/client';
import { decodeSuiPrivateKey } from '@mysten/sui/cryptography';
import { Ed25519Keypair } from '@mysten/sui/keypairs/ed25519';
import { Transaction } from '@mysten/sui/transactions';

export type Network = 'mainnet' | 'testnet' | 'devnet' | 'localnet';

export const ACTIVE_NETWORK = (process.env.NETWORK as Network) || 'testnet';

export const SUI_BIN = `sui`;

export const getActiveAddress = () => {
	return execSync(`${SUI_BIN} client active-address`, { encoding: 'utf8' }).trim();
};

/** Returns a signer based on the active address of system's sui. */
export const getSigner = () => {
	const sender = getActiveAddress();

	// `sui keytool export` reads the keystore in either of its formats. For an encrypted keystore,
	// it asks for the password in the terminal, or reads it from SUI_KEYSTORE_PASSWORD.
	const exported = JSON.parse(
		execSync(`${SUI_BIN} keytool --json export --key-identity ${sender}`, {
			encoding: 'utf8',
			stdio: ['inherit', 'pipe', 'inherit'],
		}),
	);
	const { schema, secretKey } = decodeSuiPrivateKey(exported.exportedPrivateKey);
	if (schema !== 'ED25519') {
		throw new Error(`The key of ${sender} is not an Ed25519 key`);
	}

	return Ed25519Keypair.fromSecretKey(secretKey);
};

/** Get the client for the specified network. */
export const getClient = (network: Network) => {
	return new SuiClient({ url: getFullnodeUrl(network) });
};

/** A helper to sign & execute a transaction. */
export const signAndExecute = async (txb: Transaction, network: Network) => {
	const client = getClient(network);
	const signer = getSigner();

	return client.signAndExecuteTransaction({
		transaction: txb,
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
	const txb = new Transaction();

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
	txb.transferObjects([cap], getActiveAddress());

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
