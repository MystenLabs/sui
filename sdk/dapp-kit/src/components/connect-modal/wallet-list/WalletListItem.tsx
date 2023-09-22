// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import * as styles from './WalletListItem.css.js';
import { clsx } from 'clsx';

type WalletListItemProps = {
	name: string;
	iconSrc: string;
	isSelected?: boolean;
	onClick: () => void;
};

export function WalletListItem({
	name,
	iconSrc,
	onClick,
	isSelected = false,
}: WalletListItemProps) {
	return (
		<li className={clsx(styles.container, { [styles.selectedContainer]: isSelected })}>
			<button className={styles.buttonContainer} type="button" onClick={onClick}>
				<img className={styles.walletIcon} src={iconSrc} alt="" />
				<div className={styles.walletName}>{name}</div>
			</button>
		</li>
	);
}
