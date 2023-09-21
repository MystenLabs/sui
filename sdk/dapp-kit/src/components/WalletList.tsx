// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { WalletWithRequiredFeatures } from '@mysten/wallet-standard';
import { useWallets } from '../hooks/wallet/useWallets.js';
import { WalletListItem } from './WalletListItem.js';
import * as styles from './WalletList.css.js';

type WalletListProps = {
	onPlaceholderClick: () => void;
	onSelect: (selectedWallet: WalletWithRequiredFeatures) => void;
};

export function WalletList({ onPlaceholderClick, onSelect }: WalletListProps) {
	const wallets = useWallets();
	return (
		<ul className={styles.container}>
			{wallets.length > 0 ? (
				wallets.map((wallet) => (
					<WalletListItem
						key={wallet.name}
						name={wallet.name}
						iconSrc={wallet.icon}
						onClick={() => onSelect(wallet)}
					/>
				))
			) : (
				<WalletListItem name="Sui Wallet" iconSrc="" onClick={onPlaceholderClick} />
			)}
		</ul>
	);
}
