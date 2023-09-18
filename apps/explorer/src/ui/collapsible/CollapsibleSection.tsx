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
	open?: boolean;
	onOpenChange?: (open: boolean) => void;
}

export function CollapsibleSection({
	title,
	defaultOpen = true,
	children,
	open,
	onOpenChange,
}: CollapsibleSectionProps) {
	const [isOpen, setIsOpen] = useState(defaultOpen);
	const isOpenState = typeof open === 'undefined' ? isOpen : open;
	const setOpenState = typeof onOpenChange === 'undefined' ? setIsOpen : onOpenChange;

	return (
		<Collapsible.Root
			open={isOpenState}
			onOpenChange={setOpenState}
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
							className={clsx('h-4 w-4 cursor-pointer text-gray-45', isOpenState && 'rotate-90')}
						/>
					</div>
				</Collapsible.Trigger>
			)}

			<Collapsible.Content>{children}</Collapsible.Content>
		</Collapsible.Root>
	);
}
