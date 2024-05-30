// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useInitializedGuard } from '../../hooks';
import { Text } from '../../shared/text';

export function KnownScam() {
	useInitializedGuard(true);

	return (
		<div className="bg-sui/10 rounded-20 py-15 px-10 max-w-[400px] w-full text-center flex flex-col items-center gap-10">
			<Text>known scam</Text>
		</div>
	);
}
