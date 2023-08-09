// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Text } from '@mysten/ui';

import { useEpochProgress } from '~/pages/epochs/utils';
import { ProgressCircle } from '~/ui/ProgressCircle';

export function EpochTimer() {
	const { epoch, progress, label } = useEpochProgress();
	if (!epoch) return null;
	return (
		<div className="flex w-full items-center justify-center gap-1.5 rounded-full border border-gray-45 px-2.5 py-2 shadow-notification">
			<div className="w-5 text-steel-darker">
				<ProgressCircle progress={progress} />
			</div>
			<Text variant="pBodySmall/medium" color="steel-darker">
				Epoch {epoch} in progress. {label}
			</Text>
		</div>
	);
}
