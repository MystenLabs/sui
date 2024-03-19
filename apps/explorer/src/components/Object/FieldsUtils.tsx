// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import clsx from 'clsx';
import { type ReactNode } from 'react';

import { Card } from '~/ui/Card';
import { CollapsibleSection } from '~/ui/collapsible/CollapsibleSection';

interface FieldCollapsibleProps {
	name: string | ReactNode;
	noMarginBottom: boolean;
	open: boolean;
	onOpenChange: (open: boolean) => void;
	children: ReactNode;
}

export function FieldCollapsible({
	name,
	noMarginBottom,
	children,
	open,
	onOpenChange,
}: FieldCollapsibleProps) {
	return (
		<div className={clsx(!noMarginBottom && open && 'mb-10', !noMarginBottom && !open && 'mb-4')}>
			<CollapsibleSection defaultOpen={open} title={name} open={open} onOpenChange={onOpenChange}>
				{children}
			</CollapsibleSection>
		</div>
	);
}

export function FieldsContainer({ children }: { children: ReactNode }) {
	return <div className="flex flex-col gap-10 md:flex-row md:flex-nowrap">{children}</div>;
}

export function FieldsCard({ children }: { children: ReactNode }) {
	return (
		<Card shadow bg="white" width="full">
			<div className="h-100 overflow-auto rounded-xl border-transparent bg-transparent px-2">
				{children}
			</div>
		</Card>
	);
}
