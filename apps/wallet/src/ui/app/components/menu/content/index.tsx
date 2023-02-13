// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useCallback } from 'react';
import {
    Navigate,
    Route,
    Routes,
    useLocation,
    useNavigate,
} from 'react-router-dom';

import { AccountSettings } from './AccountSettings';
import { AutoLockSettings } from './AutoLockSettings';
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
    if (!isOpen) {
        return null;
    }
    return (
        <div className="absolute flex flex-col justify-items-stretch inset-0 bg-white pt-4 pb-8 px-2.5 z-50 gap-2.5 rounded-tl-20 rounded-tr-20">
            <ErrorBoundary>
                <MainLocationContext.Provider value={mainLocation}>
                    <Routes location={menuUrl || ''}>
                        <Route path="/" element={<MenuList />} />
                        <Route path="/account" element={<AccountSettings />} />
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
                    </Routes>
                </MainLocationContext.Provider>
            </ErrorBoundary>
        </div>
    );
}

export default MenuContent;
