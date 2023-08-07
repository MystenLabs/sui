// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { BCS } from '@mysten/bcs';
import { bcs } from '@mysten/sui.js/bcs';

bcs.registerStructType('AddressParams', {
	iss: BCS.STRING,
	aud: BCS.STRING,
});

bcs.registerStructType('ZkClaim', {
	name: BCS.STRING,
	value_base64: BCS.STRING,
	index_mod_4: BCS.U8,
});

bcs.registerStructType('ZkSignature', {
	proof_points: {
		pi_a: [BCS.VECTOR, BCS.STRING],
		pi_b: [BCS.VECTOR, [BCS.VECTOR, BCS.STRING]],
		pi_c: [BCS.VECTOR, BCS.STRING],
		// TODO:
		// protocol: BCS.STRING,
		// curve: BCS.STRING,
	},
	address_seed: BCS.STRING,
	claims: [BCS.VECTOR, 'ZkClaim'],
	header_base64: BCS.STRING,
	eph_public_key: [BCS.VECTOR, BCS.U8],
	max_epoch: BCS.STRING,
	tx_sign: [BCS.VECTOR, BCS.U8],
});

export { bcs } from '@mysten/sui.js/bcs';
