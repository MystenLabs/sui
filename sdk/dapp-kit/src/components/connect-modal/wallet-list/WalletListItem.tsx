// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { clsx } from 'clsx';
import type { ReactNode } from 'react';

import * as styles from './WalletListItem.css.js';

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
				{typeof icon === 'string' ? (
					<img className={styles.walletIcon} src={icon} alt={`${name} logo`} />
				) : (
					icon
				)}
				<div className={styles.walletName}>{name}</div>
			</button>
		</li>
	);
}
