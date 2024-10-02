// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Collapsible } from '_src/ui/app/shared/collapse';
import { type ReactNode } from 'react';

type Props = {
	title: string;
	defaultOpen?: boolean;
	children: ReactNode;
};

export function TokenList({ title, defaultOpen, children }: Props) {
	return (
		<div className="flex flex-shrink-0 justify-start flex-col w-full">
			<Collapsible title={title} defaultOpen={defaultOpen}>
				{children}
			</Collapsible>
		</div>
	);
}
