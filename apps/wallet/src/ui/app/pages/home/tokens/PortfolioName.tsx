// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Text } from '_src/ui/app/shared/text';

export function PortfolioName({ name }: { name: string }) {
	return (
		<div className="flex gap-4 truncate w-full justify-center items-center">
			<div className="h-px bg-gray-45 flex-1" />
			<div className="truncate">
				<Text variant="caption" weight="semibold" color="steel-darker" truncate>
					{name} Portfolio
				</Text>
			</div>
			<div className="h-px bg-gray-45 flex-1" />
		</div>
	);
}
