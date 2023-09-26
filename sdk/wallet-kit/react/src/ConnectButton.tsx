// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Menu } from '@headlessui/react';
import { formatAddress } from '@mysten/sui.js/utils';
import { styled } from '@stitches/react';
import { ComponentProps, ReactNode, useState } from 'react';

import { ConnectModal } from './ConnectModal';
import { CheckIcon, ChevronIcon } from './utils/icons';
import { Button } from './utils/ui';
import { useWalletKit } from './WalletKitContext';

interface ConnectButtonProps extends ComponentProps<typeof Button> {
	connectText?: ReactNode;
	connectedText?: ReactNode;
}

const MenuItems = styled(Menu.Items, {
	position: 'absolute',
	right: 0,
	marginTop: '$1',
	width: 180,
	maxHeight: 200,
	overflow: 'scroll',
	borderRadius: '$buttonLg',
	backgroundColor: '$background',
	color: '$textDark',
	boxShadow: '$button',
	zIndex: 10,
	padding: '$2',
	display: 'flex',
	flexDirection: 'column',
	gap: '$2',
});

const Account = styled('button', {
	border: 0,
	display: 'flex',
	justifyContent: 'space-between',
	alignItems: 'center',
	backgroundColor: 'white',
	fontFamily: '$mono',
	padding: '$2',
	color: '#758F9E',
	cursor: 'pointer',
	textAlign: 'left',
	fontSize: 14,
	borderRadius: 3,

	'&:hover': {
		color: '#0284AD',
		backgroundColor: '#E1F3FF80',
	},

	variants: {
		active: {
			true: {
				color: '#007195',
			},
		},
	},
});

export function ConnectButton({
	connectText = 'Connect Wallet',
	connectedText,
	...props
}: ConnectButtonProps) {
	const [connectModalOpen, setConnectModalOpen] = useState(false);
	const { currentAccount, accounts, selectAccount, disconnect } = useWalletKit();

	return (
		<>
			{currentAccount ? (
				<Menu as="div" style={{ position: 'relative', display: 'inline-block' }}>
					<Menu.Button
						as={Button}
						color="connected"
						size="lg"
						css={{
							fontFamily: '$mono',
							display: 'inline-flex',
							justifyContent: 'space-between',
							alignItems: 'center',
							gap: '$2',
						}}
						type="button"
					>
						{connectedText ?? formatAddress(currentAccount.address)}
						<ChevronIcon />
					</Menu.Button>

					<MenuItems>
						{accounts.map((account) => (
							<Menu.Item key={account.address}>
								<Account
									active={account.address === currentAccount.address}
									onClick={() => selectAccount(account)}
								>
									{formatAddress(account.address)}

									{account.address === currentAccount.address && <CheckIcon />}
								</Account>
							</Menu.Item>
						))}

						<div
							style={{
								marginTop: 4,
								marginBottom: 4,
								height: 1,
								background: '#F3F6F8',
								flexShrink: 0,
							}}
						/>

						<Menu.Item>
							<Account css={{ fontFamily: '$sans' }} onClick={() => disconnect()}>
								Disconnect
							</Account>
						</Menu.Item>
					</MenuItems>
				</Menu>
			) : (
				<Button
					color="primary"
					size="lg"
					onClick={() => setConnectModalOpen(true)}
					type="button"
					{...props}
				>
					{connectText}
				</Button>
			)}

			{!currentAccount && (
				<ConnectModal open={connectModalOpen} onClose={() => setConnectModalOpen(false)} />
			)}
		</>
	);
}
