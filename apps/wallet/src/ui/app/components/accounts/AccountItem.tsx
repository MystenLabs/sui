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
					{!isActiveAccount ? (
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

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// <<<<<<< Updated upstream
// import { useResolveSuiNSName } from '@mysten/core';
// import { ArrowUpRight12, Copy12 } from '@mysten/icons';
// =======
// import { useResolveSuiNSName, useZodForm } from '@mysten/core';
// >>>>>>> Stashed changes
// import { formatAddress } from '@mysten/sui.js/utils';

// import cn from 'classnames';
// import { type ComponentProps, forwardRef, type ReactNode } from 'react';
// import { z } from 'zod';
// import { useAccounts } from '../../hooks/useAccounts';
// import { useCopyToClipboard } from '../../hooks/useCopyToClipboard';
// <<<<<<< Updated upstream
// import { useExplorerLink } from '../../hooks/useExplorerLink';
// import { IconButton } from '../IconButton';
// import { ExplorerLinkType } from '../explorer-link/ExplorerLinkType';
// =======
// import { Form } from '../../shared/forms/Form';
// >>>>>>> Stashed changes
// import { Text } from '_src/ui/app/shared/text';

// interface AccountItemProps {
// 	name?: string;
// 	address: string;
// 	icon?: ReactNode;
// 	after?: ReactNode;
// 	disabled?: boolean;
// 	gradient?: boolean;
// 	selected?: boolean; // whether the account is selected in the context of a multi-select
// 	isActiveAccount?: boolean; // whether the account is the active account in the context of the account list
// 	background?: 'gradient';
// }

// const formSchema = z.object({
// 	nickname: z.string().trim(),
// });

// type InputProps = Omit<ComponentProps<'input'>, 'className'>;

// export const Input = forwardRef<HTMLInputElement, InputProps>((props, forwardedRef) => (
// 	<input
// 		className="transition peer items-center border-none outline-none bg-transparent hover:text-hero rounded-sm text-pBody text-steel-darker font-semibold p-0 focus:bg-transparent"
// 		ref={forwardedRef}
// 		{...props}
// 	/>
// ));

// export const AccountItem = forwardRef<HTMLDivElement, AccountItemProps>(
// 	(
// 		{ background, selected, isActiveAccount, disabled, icon, name, address, after, ...props },
// 		ref,
// 	) => {
// 		const { data: accounts } = useAccounts();
// 		const { data: domainName } = useResolveSuiNSName(address);
// 		const account = accounts?.find((account) => account.address === address);
// 		const accountName = account?.nickname ?? domainName ?? formatAddress(address);
// 		const copyAddress = useCopyToClipboard(account?.address!, {
// 			copySuccessMessage: 'Address copied',
// 		});
// <<<<<<< Updated upstream
// 		const explorerHref = useExplorerLink({
// 			type: ExplorerLinkType.address,
// 			address: account?.address,
// 		});
// =======
// 		const form = useZodForm({
// 			mode: 'all',
// 			schema: formSchema,
// 			defaultValues: {
// 				nickname: accountName,
// 			},
// 		});
// 		const {
// 			register,
// 			// formState: { isSubmitting, isValid },
// 		} = form;

// 		const handleKeyPress = (e: React.KeyboardEvent<HTMLFormElement>) => {
// 			if (e.key === 'Enter') {
// 				e.preventDefault(); // Prevent form submission (if it's inside a form)
// 				// Perform your action here
// 				console.log('Enter key pressed!');
// 			}
// 		};
// >>>>>>> Stashed changes
// 		if (!account) return null;

// 		return (
// 			<div
// 				ref={ref}
// 				className={cn(
// 					'flex flex-wrap items-center gap-3 px-4 py-3 rounded-xl border border-solid border-hero/10 cursor-pointer bg-white/40 group',
// 					'hover:bg-white/80',
// 					{ 'bg-white/80 shadow-card-soft cursor-auto': selected },
// 					{ 'bg-hero/10 border-none hover:bg-white/40 shadow-none': disabled },
// 					{ 'bg-white/80': isActiveAccount },
// 					{ 'bg-gradients-graph-cards': background === 'gradient' },
// 				)}
// 				{...props}
// 			>
// 				{icon}
// 				<div className="flex flex-col gap-1 overflow-hidden items-start">
// <<<<<<< Updated upstream
// 					<Text variant="pBody" weight="semibold" color="steel-darker" truncate>
// 						{accountName}
// 					</Text>
// 					<div className="text-steel-dark flex gap-1.5 leading-none">
// =======
// 					<Form
// 						className="flex flex-col"
// 						form={form}
// 						onSubmit={() => {}}
// 						onBlur={() => console.log('do nothing')}
// 						onKeyPress={handleKeyPress}
// 					>
// 						<Input {...register('nickname')} />
// 					</Form>
// 					<button
// 						className="appearance-none outline-0 bg-transparent border-0 p-0 cursor-pointer text-steel-dark hover:text-steel-darker"
// 						onClick={copyAddress}
// 					>
// >>>>>>> Stashed changes
// 						<Text variant="subtitle" weight="semibold" truncate>
// 							{formatAddress(account.address)}
// 						</Text>
// 						<div className="opacity-0 group-hover:opacity-100 flex gap-1 duration-100">
// 							<IconButton icon={<Copy12 />} onClick={copyAddress} />
// 							{explorerHref ? (
// 								<IconButton
// 									title="View on Explorer"
// 									href={explorerHref}
// 									icon={<ArrowUpRight12 />}
// 								/>
// 							) : null}
// 						</div>
// 					</div>
// 				</div>
// 				{after}
// 			</div>
// 		);
// 	},
// );
