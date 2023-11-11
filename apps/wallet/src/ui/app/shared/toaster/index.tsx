// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useMenuIsOpen } from '_components/menu/hooks';
import { useAppSelector } from '_hooks';
import { getNavIsVisible } from '_redux/slices/app';
import cl from 'clsx';
import { Toaster as ToasterLib } from 'react-hot-toast';
import { useLocation } from 'react-router-dom';

import { Portal } from '../Portal';

export type ToasterProps = {
	bottomNavEnabled?: boolean;
};
const commonToastClasses =
	'!px-0 !py-1 !text-pBodySmall !font-medium !rounded-2lg !shadow-notification';
export function Toaster({ bottomNavEnabled = false }: ToasterProps) {
	const { pathname } = useLocation();
	const isExtraNavTabsVisible = ['/apps', '/nfts'].includes(pathname);
	const menuVisible = useMenuIsOpen();
	const isBottomNavVisible = useAppSelector(getNavIsVisible);
	const includeBottomNavSpace = !menuVisible && isBottomNavVisible && bottomNavEnabled;
	const includeExtraBottomNavSpace = includeBottomNavSpace && isExtraNavTabsVisible;
	return (
		<Portal containerId="toaster-portal-container">
			<ToasterLib
				containerClassName={cl(
					'!absolute !z-[99999] transition-all',
					includeBottomNavSpace && 'mb-nav-height',
					includeExtraBottomNavSpace && '!bottom-10',
				)}
				position="bottom-center"
				toastOptions={{
					loading: {
						icon: null,
						className: `${commonToastClasses} !bg-steel !text-white`,
					},
					error: {
						icon: null,
						className: `${commonToastClasses} !border !border-solid !border-issue-dark/20 !bg-issue-light !text-issue-dark`,
					},
					success: {
						icon: null,
						className: `${commonToastClasses} !border !border-solid !border-success-dark/20 !bg-success-light !text-success-dark`,
					},
				}}
			/>
		</Portal>
	);
}
