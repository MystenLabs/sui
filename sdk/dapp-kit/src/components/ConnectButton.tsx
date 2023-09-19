// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { ReactNode } from 'react';
import * as DropdownMenu from '@radix-ui/react-dropdown-menu';
import * as Dialog from '@radix-ui/react-dialog';
import { useCurrentAccount } from '../hooks/wallet/useCurrentAccount.js';
import { container } from './ConnectButton.css.js';

interface ConnectButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
	connectText?: ReactNode;
}

export function ConnectButton({
	connectText = 'Connect Wallet',
	...buttonProps
}: ConnectButtonProps) {
	const currentAccount = useCurrentAccount();

	return currentAccount ? (
		<DropdownMenu.Root>
			<DropdownMenu.Trigger asChild>
				<button type="button"></button>
			</DropdownMenu.Trigger>
		</DropdownMenu.Root>
	) : (
		<Dialog.Root>
			<Dialog.Trigger asChild>
				<button {...buttonProps} type="button" className={container}>
					{connectText}
				</button>
			</Dialog.Trigger>
			<Dialog.Portal>
				<Dialog.Overlay />
				<Dialog.Content>
					<Dialog.Title>Connect a Wallet</Dialog.Title>

					<ul></ul>

					<Dialog.Description />
					<Dialog.Close />
				</Dialog.Content>
			</Dialog.Portal>
		</Dialog.Root>
	);
}

// <>
// 	{currentAccount ? (
// 		<Menu as="div" style={{ position: 'relative', display: 'inline-block' }}>
// 			<Menu.Button
// 				as={Button}
// 				color="connected"
// 				size="lg"
// 				css={{
// 					fontFamily: '$mono',
// 					display: 'inline-flex',
// 					justifyContent: 'space-between',
// 					alignItems: 'center',
// 					gap: '$2',
// 				}}
// 				type="button"
// 			>
// 				{formatAddress(currentAccount.address)}
// 				<ChevronIcon />
// 			</Menu.Button>

// 			<MenuItems>
// 				{accounts.map((account) => (
// 					<Menu.Item key={account.address}>
// 						<Account
// 							active={account.address === currentAccount.address}
// 							onClick={() => selectAccount(account)}
// 						>
// 							{formatAddress(account.address)}

// 							{account.address === currentAccount.address && <CheckIcon />}
// 						</Account>
// 					</Menu.Item>
// 				))}

// 				<div
// 					style={{
// 						marginTop: 4,
// 						marginBottom: 4,
// 						height: 1,
// 						background: '#F3F6F8',
// 						flexShrink: 0,
// 					}}
// 				/>

// 				<Menu.Item>
// 					<Account css={{ fontFamily: '$sans' }} onClick={() => disconnect()}>
// 						Disconnect
// 					</Account>
// 				</Menu.Item>
// 			</MenuItems>
// 		</Menu>
// 	) : (
// 		<Button
// 			color="primary"
// 			size="lg"
// 			onClick={() => setConnectModalOpen(true)}
// 			type="button"
// 			{...props}
// 		>
// 			{connectText}
// 		</Button>
// 	)}

// 	{!currentAccount && (
// 		<ConnectModal open={connectModalOpen} onClose={() => setConnectModalOpen(false)} />
// 	)}
// </>
