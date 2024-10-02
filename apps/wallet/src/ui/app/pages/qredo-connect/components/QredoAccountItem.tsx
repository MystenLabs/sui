// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Wallet } from '_src/shared/qredo-api';
import { BadgeLabel } from '_src/ui/app/components/BadgeLabel';
import { Text } from '_src/ui/app/shared/text';
import { CheckFill16 } from '@mysten/icons';
import { formatAddress } from '@mysten/sui/utils';
import cn from 'clsx';

export type QredoAccountItemProps = Wallet & {
	selected: boolean;
	onClick: () => void;
};

export function QredoAccountItem({ selected, address, onClick, labels }: QredoAccountItemProps) {
	return (
		<div
			className="flex items-center flex-nowrap group gap-3 py-4 cursor-pointer"
			onClick={onClick}
		>
			<CheckFill16
				className={cn('w-4 h-4 text-gray-45 flex-shrink-0', {
					'text-success': selected,
					'group-hover:text-gray-60': !selected,
				})}
			/>
			<div className="flex flex-col flex-nowrap gap-2">
				<Text color={selected ? 'gray-90' : 'steel-darker'}>{formatAddress(address)}</Text>
				{labels.length ? (
					<div className="flex gap-1 flex-wrap">
						{labels.map(({ key, value }) => (
							<BadgeLabel key={key} label={value} />
						))}
					</div>
				) : null}
			</div>
		</div>
	);
}
