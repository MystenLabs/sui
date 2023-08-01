// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { formatAddress } from '@mysten/sui.js/utils';

import cn from 'classnames';
import { type ReactNode } from 'react';
import { AddressLink } from '../explorer-link';
import { Text } from '_src/ui/app/shared/text';

interface AccountItemProps {
	name?: string;
	address: string;
	icon?: ReactNode;
	after?: ReactNode;
	disabled?: boolean;
	selected?: boolean;
}

export function AccountItem({
	icon,
	after,
	disabled,
	selected,
	name,
	address,
	...props
}: AccountItemProps) {
	return (
		<div
			className={cn(
				'flex items-center gap-3 px-4 py-3 rounded-xl border border-solid border-hero/10 cursor-pointer bg-white/40 hover:bg-white/80',
				{ 'bg-white/80 shadow-card-soft': selected },
				{ 'bg-hero/10 border-none hover:bg-white/40 shadow-none pointer-events-none': disabled },
			)}
			{...props}
		>
			{icon}
			<div className="flex flex-col gap-1 overflow-hidden">
				<Text variant="pBody" weight="semibold" color="steel-darker" truncate>
					{name ?? formatAddress(address)}
				</Text>
				<AddressLink address={address} />
			</div>
			{after ? <div className="ml-auto">{after}</div> : null}
		</div>
	);
}
