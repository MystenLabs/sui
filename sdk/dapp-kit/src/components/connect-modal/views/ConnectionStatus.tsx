// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import * as styles from './GettingStarted.css.js';
import type { WalletWithRequiredFeatures } from '@mysten/wallet-standard';

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
			<img src={selectedWallet.icon} alt="" />
			<div>Opening {selectedWallet.name}</div>
			<div>{hadConnectionError ? 'Connection failed' : 'Confirm connection in the wallet...'}</div>
			{hadConnectionError ? (
				<button type="button" onClick={() => onRetryConnection(selectedWallet)}>
					Retry Connection
				</button>
			) : null}
		</div>
	);
}
