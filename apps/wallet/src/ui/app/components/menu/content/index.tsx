// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFeature } from '@growthbook/growthbook-react';
import { useCallback } from 'react';
import {
    Navigate,
    Route,
    Routes,
    useLocation,
    useNavigate,
} from 'react-router-dom';

import { ConnectLedgerModalContainer } from '../../ledger/ConnectLedgerModalContainer';
import { ImportLedgerAccounts } from '../../ledger/ImportLedgerAccounts';
import { AccountsSettings } from './AccountsSettings';
import { AutoLockSettings } from './AutoLockSettings';
import { ExportAccount } from './ExportAccount';
import { ImportPrivateKey } from './ImportPrivateKey';
import MenuList from './MenuList';
import { NetworkSettings } from './NetworkSettings';
import { ErrorBoundary } from '_components/error-boundary';
import {
    MainLocationContext,
    useMenuIsOpen,
    useMenuUrl,
    useNextMenuUrl,
} from '_components/menu/hooks';
import { useOnKeyboardEvent } from '_hooks';
import { FEATURES } from '_src/shared/experimentation/features';

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
        [isOpen, navigate, closeMenuUrl]
    );
    useOnKeyboardEvent('keydown', CLOSE_KEY_CODES, handleOnCloseMenu, isOpen);

    const isMultiAccountsEnabled = useFeature(
        FEATURES.WALLET_MULTI_ACCOUNTS
    ).on;
    const { on: isLedgerIntegrationEnabled } = useFeature(
        FEATURES.WALLET_LEDGER_INTEGRATION
    );

    if (!isOpen) {
        return null;
    }

    return (
        <div className="absolute flex flex-col justify-items-stretch inset-0 bg-white pb-8 px-2.5 z-50 rounded-tl-20 rounded-tr-20 overflow-y-auto">
            <ErrorBoundary>
                <MainLocationContext.Provider value={mainLocation}>
                    <Routes location={menuUrl || ''}>
                        <Route path="/" element={<MenuList />} />
                        {isMultiAccountsEnabled ? (
                            <>
                                <Route
                                    path="/accounts"
                                    element={<AccountsSettings />}
                                >
                                    {isLedgerIntegrationEnabled && (
                                        <Route
                                            path="connect-ledger-modal"
                                            element={
                                                <ConnectLedgerModalContainer />
                                            }
                                        />
                                    )}
                                </Route>
                                <Route
                                    path="/export/:account"
                                    element={<ExportAccount />}
                                />
                                <Route
                                    path="/import-private-key"
                                    element={<ImportPrivateKey />}
                                />
                            </>
                        ) : null}
                        <Route path="/network" element={<NetworkSettings />} />
                        <Route
                            path="/auto-lock"
                            element={<AutoLockSettings />}
                        />
                        <Route
                            path="*"
                            element={
                                <Navigate to={menuHomeUrl} replace={true} />
                            }
                        />
                        {isLedgerIntegrationEnabled && (
                            <Route
                                path="/import-ledger-accounts"
                                element={<ImportLedgerAccounts />}
                            />
                        )}
                    </Routes>
                </MainLocationContext.Provider>
            </ErrorBoundary>
        </div>
    );
}

export default MenuContent;
