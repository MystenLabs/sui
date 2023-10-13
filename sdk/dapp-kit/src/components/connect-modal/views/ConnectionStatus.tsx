// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { WalletWithRequiredFeatures } from '@mysten/wallet-standard';

import { Button } from '../../ui/Button.js';
import { Heading } from '../../ui/Heading.js';
import { Text } from '../../ui/Text.js';
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
			<div className={styles.title}>
				<Heading as="h2" size="xl">
					Opening {selectedWallet.name}
				</Heading>
			</div>
			<div className={styles.connectionStatus}>
				{hadConnectionError ? (
					<Text color="danger">Connection failed</Text>
				) : (
					<Text color="muted">Confirm connection in the wallet...</Text>
				)}
			</div>
			{hadConnectionError ? (
				<div className={styles.retryButtonContainer}>
					<Button type="button" variant="outline" onClick={() => onRetryConnection(selectedWallet)}>
						Retry Connection
					</Button>
				</div>
			) : null}
		</div>
	);
}
