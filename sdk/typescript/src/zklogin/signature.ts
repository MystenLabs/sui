// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromB64, toB64 } from '@mysten/bcs';

import { SIGNATURE_SCHEME_TO_FLAG } from '../cryptography/signature-scheme.js';
import type { ZkLoginSignature } from './bcs.js';
import { zkLoginSignature } from './bcs.js';

interface ZkLoginSignatureExtended extends Omit<ZkLoginSignature, 'userSignature'> {
	userSignature: string | ZkLoginSignature['userSignature'];
}

function getZkLoginSignatureBytes({ inputs, maxEpoch, userSignature }: ZkLoginSignatureExtended) {
	return zkLoginSignature
		.serialize(
			{
				inputs,
				maxEpoch,
				userSignature: typeof userSignature === 'string' ? fromB64(userSignature) : userSignature,
			},
			{ maxSize: 2048 },
		)
		.toBytes();
}

export function getZkLoginSignature({ inputs, maxEpoch, userSignature }: ZkLoginSignatureExtended) {
	const bytes = getZkLoginSignatureBytes({ inputs, maxEpoch, userSignature });
	const signatureBytes = new Uint8Array(bytes.length + 1);
	signatureBytes.set([SIGNATURE_SCHEME_TO_FLAG.ZkLogin]);
	signatureBytes.set(bytes, 1);
	return toB64(signatureBytes);
}

export function parseZkLoginSignature(signature: string | Uint8Array) {
	return zkLoginSignature.parse(typeof signature === 'string' ? fromB64(signature) : signature);
}
