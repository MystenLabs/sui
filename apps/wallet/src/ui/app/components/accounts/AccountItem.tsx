// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useResolveSuiNSName } from '_app/hooks/useAppResolveSuinsName';
import { Text } from '_src/ui/app/shared/text';
import { ArrowUpRight12, Copy12 } from '@mysten/icons';
import { formatAddress } from '@mysten/sui/utils';
import cn from 'clsx';
import { forwardRef, type ReactNode } from 'react';

import { getAccountBackgroundByType } from '../../helpers/accounts';
import { useAccounts } from '../../hooks/useAccounts';
import { useCopyToClipboard } from '../../hooks/useCopyToClipboard';
import { useExplorerLink } from '../../hooks/useExplorerLink';
import { ExplorerLinkType } from '../explorer-link/ExplorerLinkType';
import { IconButton } from '../IconButton';
import { EditableAccountName } from './EditableAccountName';

interface AccountItemProps {
	accountID: string;
	icon?: ReactNode;
	after?: ReactNode;
	footer?: ReactNode;
	disabled?: boolean;
	gradient?: boolean;
	selected?: boolean; // whether the account is selected in the context of a multi-select
	isActiveAccount?: boolean; // whether the account is the active account in the context of the account list
	background?: 'gradient';
	editable?: boolean;
	hideExplorerLink?: boolean;
	hideCopy?: boolean;
}

export const AccountItem = forwardRef<HTMLDivElement, AccountItemProps>(
	(
		{
			background,
			selected,
			isActiveAccount,
			disabled,
			icon,
			accountID,
			after,
			footer,
			editable,
			hideExplorerLink,
			hideCopy,
			...props
		},
		ref,
	) => {
		const { data: accounts } = useAccounts();
		const account = accounts?.find((account) => account.id === accountID);
		const domainName = useResolveSuiNSName(account?.address);
		const accountName = account?.nickname ?? domainName ?? formatAddress(account?.address || '');
		const copyAddress = useCopyToClipboard(account?.address || '', {
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
					'flex flex-col px-4 py-3 rounded-xl gap-3 border border-solid border-hero/10 cursor-pointer bg-white/40 group',
					'hover:bg-white/80 hover:border-hero/20',
					{ 'bg-white/80 shadow-card-soft cursor-auto': selected },
					{ 'bg-white/80': isActiveAccount },
					{ '!bg-hero/10 border-none hover:bg-white/40 shadow-none': disabled },
					{
						[getAccountBackgroundByType(account)]: background === 'gradient',
					},
				)}
				{...props}
			>
				<div className="flex items-center justify-start gap-3">
					<div className="self-start mt-0.5">{icon}</div>
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
								{hideCopy ? null : (
									<IconButton
										variant="transparent"
										icon={<Copy12 className="w-2.5 h-2.5" />}
										onClick={copyAddress}
									/>
								)}
								{hideExplorerLink || !explorerHref ? null : (
									<IconButton
										variant="transparent"
										title="View on Explorer"
										href={explorerHref}
										icon={<ArrowUpRight12 className="w-2.5 h-2.5" />}
										onClick={(e) => e.stopPropagation()}
									/>
								)}
							</div>
						</div>
					</div>
					{after}
				</div>
				{footer}
			</div>
		);
	},
);
