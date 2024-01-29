// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { publishPackage } from '../sui-utils';

/// A demo showing how we could publish the escrow contract
/// and our DEMO objects contract.
///
/// We're publishing both as part of our demo.
(async () => {
	await publishPackage({
		packagePath: __dirname + '/../../contracts/escrow',
		network: 'testnet',
		exportFileName: 'escrow-contract',
	});

	await publishPackage({
		packagePath: __dirname + '/../../contracts/demo',
		network: 'testnet',
		exportFileName: 'demo-contract',
	});
})();
