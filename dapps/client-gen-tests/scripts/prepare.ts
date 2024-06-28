// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { execSync, spawnSync } from 'child_process';
import { getFullnodeUrl, SuiClient } from '@mysten/sui/client';
import { decodeSuiPrivateKey, Signer } from '@mysten/sui/cryptography';
import { getFaucetHost, requestSuiFromFaucetV0 } from '@mysten/sui/faucet';
import { Ed25519Keypair } from '@mysten/sui/keypairs/ed25519';
import { Transaction } from '@mysten/sui/transactions';
import { fromB64, normalizeSuiObjectId } from '@mysten/sui/utils';

// address: `0x26742415b810da62aa75858a8a2692d38c0fdba79e024d42131e6b9ec711e069`
const PK = 'suiprivkey1qq427ctqdjt4yj6hzu7cqsr92jazswa0fk5p2960k7sk3v7glt6t5xnd5tq';
const keypair = Ed25519Keypair.fromSecretKey(decodeSuiPrivateKey(PK).secretKey);

const client = new SuiClient({ url: getFullnodeUrl('localnet') });

async function publishPackage(signer: Signer, path: string) {
	const compiledModulesAndDeps: Record<string, any> = JSON.parse(
		execSync(`cargo run --bin sui move build --dump-bytecode-as-base64`, {
			encoding: 'utf-8',
			cwd: path,
		}),
	);

	const tx = new Transaction();
	const upgradeCap = tx.publish({
		modules: compiledModulesAndDeps.modules.map((m: any) => Array.from(fromB64(m))),
		dependencies: compiledModulesAndDeps.dependencies.map((addr: string) =>
			normalizeSuiObjectId(addr),
		),
	});
	tx.transferObjects([upgradeCap], keypair.toSuiAddress());

	const res = await client.signAndExecuteTransaction({
		transaction: tx,
		signer,
		options: { showObjectChanges: true, showEvents: true },
	});

	if (res.errors) {
		throw new Error(JSON.stringify(res, undefined, 2));
	}

	return res;
}

function run(command: string, args: string[]) {
	const child = spawnSync(command, args, { stdio: 'inherit' });
	if (child.error) {
		throw child.error;
	}
	return child.status;
}

async function main() {
	await requestSuiFromFaucetV0({
		host: getFaucetHost('localnet'),
		recipient: keypair.toSuiAddress(),
	});

	// publish package
	const res = await publishPackage(keypair, './move/examples');
	let packageId = '';
	for (const change of res.objectChanges!) {
		if (change.type === 'published') {
			packageId = change.packageId;
		}
	}

	// generate client code
	run('cargo', [
		'run',
		'--bin',
		'sui',
		'client-gen',
		'-m',
		'tests/gen.toml',
		'-o',
		'tests/gen',
		'--clean',
	]);

	// replace package id in generated client code
	let indexPath = './tests/gen/examples/index.ts';
	run('sed', ['-i', `s/0x0/${packageId}/g`, indexPath]);
}

main().catch((e) => {
	console.error(e);
	process.exit(1);
});
