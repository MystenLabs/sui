// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Settings24 as SettingsIcon, HamburgerOpen24 as HamburgerOpenIcon } from '@mysten/icons';
import { useMenuIsOpen, useNextMenuUrl } from '../hooks';
import { ButtonOrLink } from '_src/ui/app/shared/utils/ButtonOrLink';

export function WalletSettingsButton() {
	const isOpen = useMenuIsOpen();
	const menuUrl = useNextMenuUrl(!isOpen, '/');
	const ButtonComponent = isOpen ? CloseButton : OpenButton;
	return <ButtonComponent to={menuUrl} />;
}

function OpenButton(props: ComponentProps<typeof ButtonOrLink>) {
	return (
		<ButtonOrLink
			className="appearance-none bg-transparent border-none cursor-pointer text-steel hover:text-hero-dark ml-auto flex items-center justify-center"
			aria-label="Open settings menu"
			{...props}
		>
			<SettingsIcon className="h-6 w-6" />
		</ButtonOrLink>
	);
}

function CloseButton(props: ComponentProps<typeof ButtonOrLink>) {
	return (
		<ButtonOrLink
			className="appearance-none bg-transparent border-none cursor-pointer text-gray-90 hover:text-hero-dark ml-auto flex items-center justify-center"
			aria-label="Close settings menu"
			{...props}
		>
			<HamburgerOpenIcon className="h-6 w-6" />
		</ButtonOrLink>
	);
}
