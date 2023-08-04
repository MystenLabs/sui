// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { BCS } from '@mysten/bcs';
import { bcs } from '@mysten/sui.js/bcs';

bcs.registerStructType('AddressParams', {
	iss: BCS.STRING,
	aud: BCS.STRING,
});

export { bcs } from '@mysten/sui.js/bcs';
