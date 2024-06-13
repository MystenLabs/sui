// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Text } from '_src/ui/app/shared/text';
import cl from 'clsx';
import { type ReactNode } from 'react';

export type LabelValueItemProps = {
	label: string;
	value: ReactNode;
	multiline?: boolean;
	parseUrl?: boolean;
};

export function LabelValueItem({ label, value, multiline = false }: LabelValueItemProps) {
	return value ? (
		<div className="flex flex-row flex-nowrap gap-1">
			<div className="flex-1 overflow-hidden">
				<Text color="steel-dark" variant="body" weight="medium" truncate>
					{label}
				</Text>
			</div>
			<div
				className={cl('max-w-[60%] break-words text-end', {
					'pr-px line-clamp-3 hover:line-clamp-none': multiline,
				})}
			>
				<Text color="steel-darker" weight="medium" truncate={!multiline}>
					{value}
				</Text>
			</div>
		</div>
	) : null;
}
