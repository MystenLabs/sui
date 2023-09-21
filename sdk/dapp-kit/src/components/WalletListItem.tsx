// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import * as styles from './WalletListItem.css.js';

type WalletListItemProps = {
	name: string;
	iconSrc: string;
	onClick: () => void;
};

export function WalletListItem({ name, iconSrc, onClick }: WalletListItemProps) {
	return (
		<li className={styles.container}>
			<button className={styles.buttonContainer} type="button" onClick={onClick}>
				<img className={styles.walletIcon} src={iconSrc} alt="" />
				<div className={styles.walletName}>{name}</div>
			</button>
		</li>
	);
}
