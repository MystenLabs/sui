// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { WalletWithRequiredFeatures } from '@mysten/wallet-standard';
import clsx from 'clsx';

import * as styles from './ConnectionStatus.css.js';

type ConnectionStatusProps = {
	selectedWallet: WalletWithRequiredFeatures;
	hadConnectionError: boolean;
	onRetryConnection: (selectedWallet: WalletWithRequiredFeatures) => void;
};

export function ConnectionStatus({
	selectedWallet,
	hadConnectionError,
	onRetryConnection,
}: ConnectionStatusProps) {
	return (
		<div className={styles.container}>
			<img
				className={styles.walletIcon}
				src={selectedWallet.icon}
				alt={`${selectedWallet.name} logo`}
			/>
			<div className={styles.walletName}>Opening {selectedWallet.name}</div>
			<div
				className={clsx(styles.connectionStatus, {
					[styles.connectionStatusWithError]: hadConnectionError,
				})}
			>
				{hadConnectionError ? 'Connection failed' : 'Confirm connection in the wallet...'}
			</div>
			{hadConnectionError ? (
				<button type="button" onClick={() => onRetryConnection(selectedWallet)}>
					Retry Connection
				</button>
			) : null}
		</div>
	);
}
