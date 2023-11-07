// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { ButtonHTMLAttributes, ReactNode } from 'react';

import { useCurrentAccount } from '../hooks/wallet/useCurrentAccount.js';
import { AccountDropdownMenu } from './AccountDropdownMenu.js';
import { ConnectModal } from './connect-modal/ConnectModal.js';
import { StyleMarker } from './styling/StyleMarker.js';
import { Button } from './ui/Button.js';

type ConnectButtonProps = {
	connectText?: ReactNode;
} & ButtonHTMLAttributes<HTMLButtonElement>;

export function ConnectButton({
	connectText = 'Connect Wallet',
	...buttonProps
}: ConnectButtonProps) {
	const currentAccount = useCurrentAccount();
	return currentAccount ? (
		<AccountDropdownMenu currentAccount={currentAccount} />
	) : (
		<ConnectModal
			trigger={
				<StyleMarker>
					<Button {...buttonProps}>{connectText}</Button>
				</StyleMarker>
			}
		/>
	);
}
