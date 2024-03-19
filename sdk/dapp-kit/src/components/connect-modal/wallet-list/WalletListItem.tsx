// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { clsx } from 'clsx';
import type { ReactNode } from 'react';

import { Heading } from '../../ui/Heading.js';
import * as styles from './WalletListItem.css.js';

type WalletListItemProps = {
	name: string;
	icon: ReactNode;
	isSelected?: boolean;
	onClick: () => void;
};

export function WalletListItem({ name, icon, onClick, isSelected = false }: WalletListItemProps) {
	return (
		<li className={styles.container}>
			<button
				className={clsx(styles.walletItem, { [styles.selectedWalletItem]: isSelected })}
				type="button"
				onClick={onClick}
			>
				{typeof icon === 'string' ? (
					<img className={styles.walletIcon} src={icon} alt={`${name} logo`} />
				) : (
					icon
				)}
				<Heading size="md" truncate asChild>
					<div>{name}</div>
				</Heading>
			</button>
		</li>
	);
}
