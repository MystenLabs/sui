// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useResolveSuiNSName } from '@mysten/core';
import { ArrowUpRight12, Copy12 } from '@mysten/icons';
import { formatAddress } from '@mysten/sui.js/utils';

import cn from 'classnames';
import { forwardRef, type ReactNode } from 'react';
import { EditableAccountName } from './EditableAccountName';
import { useAccounts } from '../../hooks/useAccounts';
import { useCopyToClipboard } from '../../hooks/useCopyToClipboard';
import { useExplorerLink } from '../../hooks/useExplorerLink';
import { IconButton } from '../IconButton';
import { ExplorerLinkType } from '../explorer-link/ExplorerLinkType';
import { Text } from '_src/ui/app/shared/text';

interface AccountItemProps {
	name?: string;
	address: string;
	icon?: ReactNode;
	after?: ReactNode;
	disabled?: boolean;
	gradient?: boolean;
	selected?: boolean; // whether the account is selected in the context of a multi-select
	isActiveAccount?: boolean; // whether the account is the active account in the context of the account list
	background?: 'gradient';
	editable?: boolean;
}

export const AccountItem = forwardRef<HTMLDivElement, AccountItemProps>(
	(
		{
			background,
			selected,
			isActiveAccount,
			disabled,
			icon,
			name,
			address,
			after,
			editable,
			...props
		},
		ref,
	) => {
		const { data: accounts } = useAccounts();
		const { data: domainName } = useResolveSuiNSName(address);
		const account = accounts?.find((account) => account.address === address);
		const accountName = account?.nickname ?? domainName ?? formatAddress(address);
		const copyAddress = useCopyToClipboard(account?.address!, {
			copySuccessMessage: 'Address copied',
		});
		const explorerHref = useExplorerLink({
			type: ExplorerLinkType.address,
			address: account?.address,
		});
		if (!account) return null;

		return (
			<div
				ref={ref}
				className={cn(
					'flex flex-wrap items-center gap-3 px-4 py-3 rounded-xl border border-solid border-hero/10 cursor-pointer bg-white/40 group',
					'hover:bg-white/80',
					{ 'bg-white/80 shadow-card-soft cursor-auto': selected },
					{ 'bg-hero/10 border-none hover:bg-white/40 shadow-none': disabled },
					{ 'bg-white/80': isActiveAccount },
					{ 'bg-gradients-graph-cards': background === 'gradient' },
				)}
				{...props}
			>
				{icon}
				<div className="flex flex-col gap-1 overflow-hidden items-start">
					{!isActiveAccount && !editable ? (
						<Text variant="pBody" weight="semibold" color="steel-darker" truncate>
							{accountName}
						</Text>
					) : (
						<EditableAccountName accountID={account.id} name={accountName} />
					)}
					<div className="text-steel-dark flex gap-1.5 leading-none">
						<Text variant="subtitle" weight="semibold" truncate>
							{formatAddress(account.address)}
						</Text>
						<div className="opacity-0 group-hover:opacity-100 flex gap-1 duration-100">
							<IconButton icon={<Copy12 />} onClick={copyAddress} />
							{explorerHref ? (
								<IconButton
									title="View on Explorer"
									href={explorerHref}
									icon={<ArrowUpRight12 />}
								/>
							) : null}
						</div>
					</div>
				</div>
				{after}
			</div>
		);
	},
);
