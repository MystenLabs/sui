// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { IdentifierRecord, SuiFeatures, SuiSignMessageFeature } from '@mysten/wallet-standard';

export const signMessageFeature: SuiSignMessageFeature = {
	'sui:signMessage': {
		version: '1.0.0',
		signMessage: vi.fn(),
	},
};

export const superCoolFeature: IdentifierRecord<unknown> = {
	'my-dapp:super-cool-feature': {
		version: '1.0.0',
		superCoolFeature: vi.fn(),
	},
};

export const suiFeatures: SuiFeatures = {
	...signMessageFeature,
	'sui:signPersonalMessage': {
		version: '1.0.0',
		signPersonalMessage: vi.fn(),
	},
	'sui:signTransactionBlock': {
		version: '1.0.0',
		signTransactionBlock: vi.fn(),
	},
	'sui:signTransaction': {
		version: '2.0.0',
		signTransaction: vi.fn(),
	},
	'sui:signAndExecuteTransactionBlock': {
		version: '1.0.0',
		signAndExecuteTransactionBlock: vi.fn(),
	},
	'sui:signAndExecuteTransaction': {
		version: '2.0.0',
		signAndExecuteTransaction: vi.fn(),
	},
	'sui:reportTransactionEffects': {
		version: '1.0.0',
		reportTransactionEffects: vi.fn(),
	},
};
