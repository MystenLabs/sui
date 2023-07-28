// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useResolveSuiNSName } from '@mysten/core';
import SvgSocialGoogle24 from '@mysten/icons/src/SocialGoogle24';
import { formatAddress } from '@mysten/sui.js/utils';
import * as ToggleGroup from '@radix-ui/react-toggle-group';
import { useState } from 'react';
import { AccountItem } from './AccountItem';
import { LockUnlockButton } from './LockUnlockButton';
import { useActiveAddress } from '../../hooks';

export function AccountListItem({ address }: { address: string }) {
	const activeAddress = useActiveAddress();
	const { data: domainName } = useResolveSuiNSName(address);
	// todo: remove this when we implement account locking / unlocking
	const [locked, setLocked] = useState(false);

	return (
		<ToggleGroup.Item asChild value={address!}>
			<AccountItem
				icon={<SvgSocialGoogle24 className="h-4 w-4" />}
				name={domainName ?? formatAddress(address!)}
				selected={address === activeAddress}
				after={
					<div className="flex items-center justify-center">
						<LockUnlockButton
							isLocked={locked}
							onClick={() => {
								setLocked((prev) => !prev);
							}}
						/>
					</div>
				}
				address={address!}
			/>
		</ToggleGroup.Item>
	);
}
