// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Text } from '_app/shared/text';
import type { ReactNode } from 'react';

export type FormLabelProps = {
	label?: ReactNode;
	children: ReactNode;
};

export function FormLabel({ label, children }: FormLabelProps) {
	return (
		<label className="flex flex-col flex-nowrap gap-2.5">
			{label && (
				<div className="pl-2.5">
					<Text variant="body" color="steel-darker" weight="semibold">
						{label}
					</Text>
				</div>
			)}
			{children}
		</label>
	);
}
