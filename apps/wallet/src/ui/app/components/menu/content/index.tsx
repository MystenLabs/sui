// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ErrorBoundary } from '_components/error-boundary';
import {
	MainLocationContext,
	useMenuIsOpen,
	useMenuUrl,
	useNextMenuUrl,
} from '_components/menu/hooks';
import { useOnKeyboardEvent } from '_hooks';
import { useCallback } from 'react';
import type { MouseEvent } from 'react';
import { Navigate, Route, Routes, useLocation, useNavigate } from 'react-router-dom';

import { AutoLockAccounts } from './AutoLockAccounts';
import { MoreOptions } from './MoreOptions';
import { NetworkSettings } from './NetworkSettings';
import WalletSettingsMenuList from './WalletSettingsMenuList';

const CLOSE_KEY_CODES: string[] = ['Escape'];

function MenuContent() {
	const mainLocation = useLocation();
	const isOpen = useMenuIsOpen();
	const menuUrl = useMenuUrl();
	const menuHomeUrl = useNextMenuUrl(true, '/');
	const closeMenuUrl = useNextMenuUrl(false);
	const navigate = useNavigate();
	const handleOnCloseMenu = useCallback(
		(e: KeyboardEvent | MouseEvent<HTMLDivElement>) => {
			if (isOpen) {
				e.preventDefault();
				navigate(closeMenuUrl);
			}
		},
		[isOpen, navigate, closeMenuUrl],
	);

	useOnKeyboardEvent('keydown', CLOSE_KEY_CODES, handleOnCloseMenu, isOpen);
	if (!isOpen) {
		return null;
	}

	return (
		<div className="absolute flex flex-col justify-items-stretch inset-0 bg-white pb-8 px-2.5 z-50 rounded-t-xl overflow-y-auto">
			<ErrorBoundary>
				<MainLocationContext.Provider value={mainLocation}>
					<Routes location={menuUrl || ''}>
						<Route path="/" element={<WalletSettingsMenuList />} />
						<Route path="/network" element={<NetworkSettings />} />
						<Route path="/auto-lock" element={<AutoLockAccounts />} />
						<Route path="/more-options" element={<MoreOptions />} />
						<Route path="*" element={<Navigate to={menuHomeUrl} replace={true} />} />
					</Routes>
				</MainLocationContext.Provider>
			</ErrorBoundary>
		</div>
	);
}

export default MenuContent;
