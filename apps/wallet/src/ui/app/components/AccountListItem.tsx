// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useResolveSuiNSName } from '@mysten/core';
import { Check12, Copy12 } from '@mysten/icons';
import { formatAddress } from '@mysten/sui.js';

import { AccountBadge } from './AccountBadge';
import { useActiveAddress } from '../hooks/useActiveAddress';
import { useCopyToClipboard } from '../hooks/useCopyToClipboard';
import { Text } from '../shared/text';
import { type SerializedAccount } from '_src/background/keyring/Account';

export type AccountItemProps = {
	account: SerializedAccount;
	onAccountSelected: (address: SerializedAccount) => void;
};

export function AccountListItem({ account, onAccountSelected }: AccountItemProps) {
	const { address, type } = account;
	const activeAddress = useActiveAddress();
	const copy = useCopyToClipboard(address, {
		copySuccessMessage: 'Address Copied',
	});
	const { data: domainName } = useResolveSuiNSName(address);

	return (
		<li>
			<button
				className="appearance-none bg-transparent border-0 w-full flex p-2.5 items-center gap-2.5 rounded-md hover:bg-sui/10 cursor-pointer focus-visible:ring-1 group transition-colors text-left"
				onClick={() => {
					onAccountSelected(account);
				}}
			>
				<div className="flex items-center gap-2 flex-1 min-w-0">
					<div className="min-w-0">
						<Text color="steel-darker" variant="bodySmall" truncate mono>
							{domainName ?? formatAddress(address)}
						</Text>
					</div>
					<AccountBadge accountType={type} />
				</div>
				{activeAddress === address ? <Check12 className="text-success" /> : null}
				<Copy12
					className="text-gray-60 group-hover:text-steel transition-colors hover:!text-hero-dark"
					onClick={copy}
				/>
			</button>
		</li>
	);
}
