// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import clsx from 'clsx';
import { type ReactNode, useState } from 'react';

import { CollapsibleSection } from '~/ui/collapsible/CollapsibleSection';

export function FieldsCollapsible({
	name,
	children,
}: {
	name: string | ReactNode;
	children?: ReactNode;
}) {
	const [open, setOpen] = useState(true);

	return (
		<div className={clsx(open ? 'mb-10' : 'mb-4')}>
			<CollapsibleSection title={name} onOpenChange={setOpen}>
				{children}
			</CollapsibleSection>
		</div>
	);
}
