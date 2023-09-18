// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Text } from '@mysten/ui';

export function AddressResultEmptyState({ copy }: { copy: string }) {
	return (
		<div className="flex h-20 items-center justify-center md:h-full">
			<Text variant="body/medium" color="steel-dark">
				{copy}
			</Text>
		</div>
	);
}
