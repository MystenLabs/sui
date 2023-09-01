// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { ChevronRight12 } from '@mysten/icons';
import { Text } from '@mysten/ui';
import * as Collapsible from '@radix-ui/react-collapsible';
import clsx from 'clsx';
import { type ReactNode, useState } from 'react';

import { Divider } from '~/ui/Divider';

export interface CollapsibleSectionProps {
	children: ReactNode;
	defaultOpen?: boolean;
	title?: string | ReactNode;
	onOpenChange?: (open: boolean) => void;
}

export function CollapsibleSection({
	title,
	defaultOpen = true,
	onOpenChange,
	children,
}: CollapsibleSectionProps) {
	const [open, setOpen] = useState(defaultOpen);

	const handleSetOpen = (open: boolean) => {
		setOpen(open);
		if (onOpenChange) {
			onOpenChange(open);
		}
	};

	return (
		<Collapsible.Root
			open={open}
			onOpenChange={handleSetOpen}
			className="flex w-full flex-col gap-3"
		>
			{title && (
				<Collapsible.Trigger>
					<div className="flex items-center gap-2">
						{typeof title === 'string' ? (
							<Text color="steel-darker" variant="body/semibold">
								{title}
							</Text>
						) : (
							title
						)}
						<Divider />
						<ChevronRight12
							className={clsx('h-4 w-4 cursor-pointer text-gray-45', open && 'rotate-90')}
						/>
					</div>
				</Collapsible.Trigger>
			)}

			<Collapsible.Content>{children}</Collapsible.Content>
		</Collapsible.Root>
	);
}
