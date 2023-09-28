// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getActiveNetworkSuiClient } from '_shared/sui-client';
import { DeepBookClient } from '@mysten/deepbook';

export async function getDeepbookClient(): Promise<DeepBookClient> {
	const suiClient = await getActiveNetworkSuiClient();
	return new DeepBookClient(suiClient);
}
