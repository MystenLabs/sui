// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { ReactNode } from 'react';
import { useCurrentWallet } from '../hooks/wallet/useCurrentWallet.js';
import { AccountDropdownMenu } from './AccountDropdownMenu.js';
import { ConnectModal } from './connect-modal/ConnectModal.js';

type ConnectButtonProps = {
	connectText?: ReactNode;
};

export function ConnectButton({ connectText = 'Connect Wallet' }: ConnectButtonProps) {
	const currentWallet = useCurrentWallet();
	return currentWallet ? <AccountDropdownMenu /> : <ConnectModal triggerButton={connectText} />;
}
