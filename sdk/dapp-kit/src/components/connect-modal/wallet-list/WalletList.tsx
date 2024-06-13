// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { WalletWithRequiredFeatures } from '@mysten/wallet-standard';

import { useWallets } from '../../../hooks/wallet/useWallets.js';
import { getWalletUniqueIdentifier } from '../../../utils/walletUtils.js';
import { SuiIcon } from '../../icons/SuiIcon.js';
import * as styles from './WalletList.css.js';
import { WalletListItem } from './WalletListItem.js';

type WalletListProps = {
	selectedWalletName?: string;
	onPlaceholderClick: () => void;
	onSelect: (wallet: WalletWithRequiredFeatures) => void;
	walletFilter?: (wallet: WalletWithRequiredFeatures) => boolean;
};

export function WalletList({
	selectedWalletName,
	onPlaceholderClick,
	onSelect,
	walletFilter,
}: WalletListProps) {
	const wallets = useWallets();
	const filteredWallets = walletFilter ? wallets.filter((wallet) => walletFilter(wallet)) : wallets;
	return (
		<ul className={styles.container}>
			{filteredWallets.length > 0 ? (
				filteredWallets.map((wallet) => (
					<WalletListItem
						key={getWalletUniqueIdentifier(wallet)}
						name={wallet.name}
						icon={wallet.icon}
						isSelected={getWalletUniqueIdentifier(wallet) === selectedWalletName}
						onClick={() => onSelect(wallet)}
					/>
				))
			) : (
				<WalletListItem
					name="Sui Wallet"
					icon={<SuiIcon />}
					onClick={onPlaceholderClick}
					isSelected
				/>
			)}
		</ul>
	);
}
