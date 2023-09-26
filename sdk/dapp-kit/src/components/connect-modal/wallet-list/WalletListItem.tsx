// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { clsx } from 'clsx';

import * as styles from './WalletListItem.css.js';
import type { ReactNode } from 'react';

type WalletListItemProps = {
	name: string;
	icon: ReactNode;
	isSelected?: boolean;
	onClick: () => void;
};
export function WalletListItem({ name, icon, onClick, isSelected = false }: WalletListItemProps) {
	return (
		<li className={clsx(styles.container, { [styles.selectedContainer]: isSelected })}>
			<button className={styles.buttonContainer} type="button" onClick={onClick}>
				{typeof icon === 'string' ? <img className={styles.walletIcon} src={icon} alt="" /> : icon}
				<div className={styles.walletName}>{name}</div>
			</button>
		</li>
	);
}
