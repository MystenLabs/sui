// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Sui } from '@mysten/icons';
import { formatAddress } from '@mysten/sui.js/utils';

import cn from 'classnames';
import { forwardRef, type ReactNode } from 'react';
import { useAccounts } from '../../hooks/useAccounts';
import { AddressLink } from '../explorer-link';
import { Text } from '_src/ui/app/shared/text';

interface AccountItemProps {
	name?: string;
	address: string;
	icon?: ReactNode;
	after?: ReactNode;
	disabled?: boolean;
	gradient?: boolean;
	selected?: boolean;
	// todo: extract into variants if possible
	background?: 'gradient';
}

// todo: remove this when we have real account icons
function PlaceholderRoundedSui() {
	return (
		<div className="bg-sui-primaryBlue2023 rounded-full text-white h-4 w-4 flex items-center justify-center">
			<Sui />
		</div>
	);
}

export const AccountItem = forwardRef<HTMLDivElement, AccountItemProps>(
	(
		{
			background,
			selected,
			disabled,
			icon = <PlaceholderRoundedSui />,
			name,
			address,
			after,
			...props
		},
		ref,
	) => {
		const { data: accounts } = useAccounts();
		const account = accounts?.find((account) => account.address === address);
		return (
			<div
				ref={ref}
				className={cn(
					'flex flex-wrap items-center gap-3 px-4 py-3 rounded-xl border border-solid border-hero/10 cursor-pointer bg-white/40 hover:bg-white/80 group',
					{ 'bg-white/80 shadow-card-soft': selected },
					{ 'bg-hero/10 border-none hover:bg-white/40 shadow-none pointer-events-none': disabled },
					{ 'bg-gradients-graph-cards': background === 'gradient' },
				)}
				{...props}
			>
				{icon}
				<div className="flex flex-col gap-1 overflow-hidden">
					<Text variant="pBody" weight="semibold" color="steel-darker" truncate>
						{account?.nickname || name || formatAddress(address)}
					</Text>
					<AddressLink address={address} />
				</div>
				{after}
			</div>
		);
	},
);
