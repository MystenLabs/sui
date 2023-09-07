// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { BCS, fromB64, toB64 } from '@mysten/bcs';
import { SIGNATURE_SCHEME_TO_FLAG } from '@mysten/sui.js/cryptography';
import { bcs } from '@mysten/sui.js/bcs';

export const zkBcs = new BCS(bcs);

zkBcs.registerStructType('AddressParams', {
	iss: BCS.STRING,
});

zkBcs.registerStructType('ZkClaim', {
	name: BCS.STRING,
	value_base64: BCS.STRING,
	index_mod_4: BCS.U8,
});

type Claim = {
	name: string;
	value_base64: string;
	index_mod_4: number;
};

export interface ProofPoints {
	pi_a: string[];
	pi_b: string[][];
	pi_c: string[];
}

export interface ZkSignatureInputs {
	proof_points: ProofPoints;
	address_seed: string;
	claims: Claim[];
	header_base64: string;
}

export interface ZkSignature {
	inputs: ZkSignatureInputs;
	maxEpoch: number;
	userSignature: string | Uint8Array;
}

zkBcs.registerStructType('ZkSignature', {
	inputs: {
		proof_points: {
			pi_a: [BCS.VECTOR, BCS.STRING],
			pi_b: [BCS.VECTOR, [BCS.VECTOR, BCS.STRING]],
			pi_c: [BCS.VECTOR, BCS.STRING],
		},
		address_seed: BCS.STRING,
		claims: [BCS.VECTOR, 'ZkClaim'],
		header_base64: BCS.STRING,
	},
	max_epoch: BCS.U64,
	user_signature: [BCS.VECTOR, BCS.U8],
});

function getZkSignatureBytes({ inputs, maxEpoch, userSignature }: ZkSignature) {
	return zkBcs
		.ser(
			'ZkSignature',
			{
				inputs,
				max_epoch: maxEpoch,
				user_signature: typeof userSignature === 'string' ? fromB64(userSignature) : userSignature,
			},
			{ maxSize: 2048 },
		)
		.toBytes();
}

export function getZkSignature({ inputs, maxEpoch, userSignature }: ZkSignature) {
	const bytes = getZkSignatureBytes({ inputs, maxEpoch, userSignature });
	const signatureBytes = new Uint8Array(bytes.length + 1);
	signatureBytes.set([SIGNATURE_SCHEME_TO_FLAG['Zk']]);
	signatureBytes.set(bytes, 1);
	return toB64(signatureBytes);
}
