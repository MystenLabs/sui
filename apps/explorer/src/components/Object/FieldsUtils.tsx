// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import clsx from 'clsx';
import { forwardRef, type ReactNode, type Ref } from 'react';

import { Card } from '~/ui/Card';
import { CollapsibleSection } from '~/ui/collapsible/CollapsibleSection';

interface FieldCollapsibleProps {
	name: string | ReactNode;
	noMarginBottom: boolean;
	open: boolean;
	setOpen: (open: boolean) => void;
	children: ReactNode;
}

export function FieldCollapsible({
	name,
	noMarginBottom,
	children,
	open,
	setOpen,
}: FieldCollapsibleProps) {
	return (
		<div className={clsx(!noMarginBottom && open && 'mb-10', !noMarginBottom && !open && 'mb-4')}>
			<CollapsibleSection
				defaultOpen={open}
				title={name}
				externalControl={{
					open,
					setOpen,
				}}
			>
				{children}
			</CollapsibleSection>
		</div>
	);
}

export function FieldsContainer({ children }: { children: ReactNode }) {
	return <div className="flex flex-col gap-10 md:flex-row md:flex-nowrap">{children}</div>;
}

export const FieldsCard = forwardRef(
	({ children }: { children: ReactNode }, ref: Ref<HTMLDivElement>) => (
		<Card shadow bg="white" width="full">
			<div
				ref={ref}
				className="h-100 overflow-auto rounded-xl border-transparent bg-transparent px-2"
			>
				{children}
			</div>
		</Card>
	),
);
