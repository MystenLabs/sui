// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { execSync } from 'child_process';
import fs from 'fs';
import { TransactionBlock } from '@mysten/sui.js/transactions';

import { CONFIG } from '../config';
import { getActiveAddress, signAndExecute, SUI_BIN } from '../sui-utils';

const publishPackage = async () => {
	const txb = new TransactionBlock();
	const packagePath = __dirname + '/../../contract';

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

	const results = await signAndExecute(txb, CONFIG.NETWORK);

	// @ts-ignore-next-line
	const packageId = results.objectChanges?.find((x) => x.type === 'published')?.packageId;

	// save to an env file
	fs.writeFileSync(
		'contracts.json',
		JSON.stringify({
			packageId,
		}),
		{ encoding: 'utf8', flag: 'w' },
	);
};

publishPackage();
