// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useCallback } from 'react';
import { Navigate, Route, Routes, useLocation, useNavigate } from 'react-router-dom';

import { AccountsSettings } from './AccountsSettings';
import { AutoLockSettings } from './AutoLockSettings';
import { ExportAccount } from './ExportAccount';
import { ImportPrivateKey } from './ImportPrivateKey';
import MenuList from './MenuList';
import { NetworkSettings } from './NetworkSettings';
import { ConnectLedgerModalContainer } from '../../ledger/ConnectLedgerModalContainer';
import { ImportLedgerAccounts } from '../../ledger/ImportLedgerAccounts';
import { ErrorBoundary } from '_components/error-boundary';
import {
	MainLocationContext,
	useMenuIsOpen,
	useMenuUrl,
	useNextMenuUrl,
} from '_components/menu/hooks';
import { RecoveryPassphrase } from '_components/recovery-passphrase/RecoveryPassphrase';
import { useOnKeyboardEvent } from '_hooks';

import type { MouseEvent } from 'react';

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
		<div className="absolute flex flex-col justify-items-stretch inset-0 bg-white pb-8 px-2.5 z-50 rounded-tl-20 rounded-tr-20 overflow-y-auto">
			<ErrorBoundary>
				<MainLocationContext.Provider value={mainLocation}>
					<Routes location={menuUrl || ''}>
						<Route path="/" element={<MenuList />} />
						<Route path="/accounts" element={<AccountsSettings />}>
							<Route path="connect-ledger-modal" element={<ConnectLedgerModalContainer />} />
						</Route>
						<Route path="/export/:account" element={<ExportAccount />} />
						<Route path="/import-private-key" element={<ImportPrivateKey />} />
						<Route path="/network" element={<NetworkSettings />} />
						<Route path="/auto-lock" element={<AutoLockSettings />} />
						<Route path="*" element={<Navigate to={menuHomeUrl} replace={true} />} />
						<Route path="/import-ledger-accounts" element={<ImportLedgerAccounts />} />
						<Route path="/recovery-passphrase" element={<RecoveryPassphrase />} />
					</Routes>
				</MainLocationContext.Provider>
			</ErrorBoundary>
		</div>
	);
}

export default MenuContent;
