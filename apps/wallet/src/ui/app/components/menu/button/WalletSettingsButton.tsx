// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ButtonOrLink } from '_src/ui/app/shared/utils/ButtonOrLink';
import { HamburgerOpen24 as HamburgerOpenIcon, Settings24 as SettingsIcon } from '@mysten/icons';
import { cx } from 'class-variance-authority';

import { useMenuIsOpen, useNextMenuUrl } from '../hooks';

export function WalletSettingsButton() {
	const isOpen = useMenuIsOpen();
	const menuUrl = useNextMenuUrl(!isOpen, '/');
	const IconComponent = isOpen ? HamburgerOpenIcon : SettingsIcon;

	return (
		<ButtonOrLink
			className={cx(
				'appearance-none bg-transparent border-none cursor-pointer hover:text-hero-dark ml-auto flex items-center justify-center',
				{ 'text-steel': !isOpen, 'text-gray-90': isOpen },
			)}
			aria-label={isOpen ? 'Close settings menu' : 'Open settings menu'}
			to={menuUrl}
		>
			<IconComponent className="h-6 w-6" />
		</ButtonOrLink>
	);
}
