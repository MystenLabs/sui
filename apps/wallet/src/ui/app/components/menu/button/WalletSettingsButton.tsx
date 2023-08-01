// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Settings24 as SettingsIcon, HamburgerOpen24 as HamburgerOpenIcon } from '@mysten/icons';
import { useMenuIsOpen, useNextMenuUrl } from '../hooks';
import { ButtonOrLink } from '_src/ui/app/shared/utils/ButtonOrLink';

export function WalletSettingsButton() {
	const isOpen = useMenuIsOpen();
	const menuUrl = useNextMenuUrl(!isOpen, '/');

	return (
		<ButtonOrLink
			className="appearance-none bg-transparent border-none cursor-pointer text-steel-dark hover:text-hero-dark ml-auto flex items-center justify-center"
			to={menuUrl}
		>
			{isOpen ? <HamburgerOpenIcon className="h-6 w-6" /> : <SettingsIcon className="h-6 w-6" />}
		</ButtonOrLink>
	);
}
