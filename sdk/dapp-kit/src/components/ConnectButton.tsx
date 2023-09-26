// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { ReactNode } from 'react';

import { AccountDropdownMenu } from './AccountDropdownMenu.js';
import { ConnectModal } from './connect-modal/ConnectModal.js';
import { useCurrentAccount } from '../hooks/wallet/useCurrentAccount.js';

type ConnectButtonProps = {
	connectText?: ReactNode;
};

export function ConnectButton({ connectText = 'Connect Wallet' }: ConnectButtonProps) {
	const currentAccount = useCurrentAccount();
	return currentAccount ? (
		<AccountDropdownMenu currentAccount={currentAccount} />
	) : (
		<ConnectModal trigger={connectText} />
	);
}
