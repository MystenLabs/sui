// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import networkEnv from '_src/background/NetworkEnv';
import { getFromSessionStorage, setToSessionStorage } from '_src/background/storage-utils';
import { type NetworkEnvType } from '_src/shared/api-env';
import { getActiveNetworkSuiClient } from '_src/shared/sui-client';

type EpochCacheInfo = {
	epoch: number;
	epochEndTimestamp: number;
};

function epochCacheKey(network: NetworkEnvType) {
	return `epoch_cache_${network.env}-${network.customRpcUrl}`;
}

async function getCurrentEpochRequest(): Promise<EpochCacheInfo> {
	const suiClient = await getActiveNetworkSuiClient();
	const { epoch, epochDurationMs, epochStartTimestampMs } =
		await suiClient.getLatestSuiSystemState();
	return {
		epoch: Number(epoch),
		epochEndTimestamp: Number(epochStartTimestampMs) + Number(epochDurationMs),
	};
}

export async function getCurrentEpoch() {
	const activeNetwork = await networkEnv.getActiveNetwork();
	const cache = await getFromSessionStorage<EpochCacheInfo>(epochCacheKey(activeNetwork));
	if (cache && Date.now() <= cache.epochEndTimestamp) {
		return cache.epoch;
	}
	const { epoch, epochEndTimestamp } = await getCurrentEpochRequest();
	const newCache: EpochCacheInfo = {
		epoch,
		epochEndTimestamp:
			// add some extra time to existing epochEndTimestamp to avoid making repeating requests while epoch is changing
			cache?.epoch === epoch ? cache.epochEndTimestamp + 5 * 1000 : epochEndTimestamp,
	};
	await setToSessionStorage(epochCacheKey(activeNetwork), newCache);
	return epoch;
}
