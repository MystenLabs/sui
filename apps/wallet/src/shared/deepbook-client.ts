// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { DeepBookClient } from '@mysten/deepbook';
import { getActiveNetworkSuiClient } from '_shared/sui-client';

export async function getDeepbookClient() {
	const suiClient = await getActiveNetworkSuiClient();
	return new DeepBookClient(suiClient);
}
