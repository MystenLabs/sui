// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useResolveSuiNSName } from '@mysten/core';
import { formatAddress } from '@mysten/sui.js/utils';

import { type ReactNode, useState } from 'react';
import { AccountItem } from './AccountItem';
import { LockUnlockButton } from './LockUnlockButton';
import { useActiveAddress } from '../../hooks';

type AccountListItemProps = {
	address: string;
	icon?: ReactNode;
	handleLockAccount?: () => void;
	handleUnlockAccount?: () => void;
	editable?: boolean;
};

export function AccountListItem({
	address,
	icon,
	handleLockAccount,
	handleUnlockAccount,
}: AccountListItemProps) {
	const activeAddress = useActiveAddress();
	const { data: domainName } = useResolveSuiNSName(address);
	// todo: remove this when we implement account locking / unlocking
	const [locked, setLocked] = useState(false);
	return (
		<AccountItem
			icon={icon}
			name={domainName ?? formatAddress(address)}
			selected={address === activeAddress}
			after={
				<div className="ml-auto">
					<div className="flex items-center justify-center">
						<LockUnlockButton
							isLocked={locked}
							onClick={() => {
								// todo: this state will be managed elsewhere
								if (locked) handleUnlockAccount?.();
								if (!locked) handleLockAccount?.();
								setLocked((prev) => !prev);
							}}
						/>
					</div>
				</div>
			}
			address={address!}
		/>
	);
}
