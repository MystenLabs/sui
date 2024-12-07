// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import '@mysten/sui/zklogin';

import type { ComputeZkLoginAddressOptions } from '@mysten/sui/zklogin';
import {
	computeZkLoginAddress as suiComputeZkLoginAddress,
	jwtToAddress as suiJwtToAddress,
} from '@mysten/sui/zklogin';

export type { ComputeZkLoginAddressOptions } from '@mysten/sui/zklogin';

export {
	/** @deprecated, use `import { genAddressSeed } from '@mysten/sui/zklogin';` instead */
	genAddressSeed,
	/** @deprecated, use `import { generateNonce } from '@mysten/sui/zklogin';` instead */
	generateNonce,
	/** @deprecated, use `import { generateRandomness } from '@mysten/sui/zklogin';` instead */
	generateRandomness,
	/** @deprecated, use `import { getExtendedEphemeralPublicKey } from '@mysten/sui/zklogin';` instead */
	getExtendedEphemeralPublicKey,
	/** @deprecated, use `import { getZkLoginSignature } from '@mysten/sui/zklogin';` instead */
	getZkLoginSignature,
	/** @deprecated, use `import { hashASCIIStrToField } from '@mysten/sui/zklogin';` instead */
	hashASCIIStrToField,
	/** @deprecated, use `import { poseidonHash } from '@mysten/sui/zklogin';` instead */
	poseidonHash,
} from '@mysten/sui/zklogin';

/** @deprecated, use `import { parseZkLoginSignature } from '@mysten/sui/zklogin';` instead */
export function computeZkLoginAddress(options: ComputeZkLoginAddressOptions) {
	return suiComputeZkLoginAddress({
		...options,
		legacyAddress: true,
	});
}

/** @deprecated, use `import { jwtToAddress } from '@mysten/sui/zklogin';` instead */
export function jwtToAddress(jwt: string, userSalt: string | bigint, legacyAddress = true) {
	return suiJwtToAddress(jwt, userSalt, legacyAddress);
}
