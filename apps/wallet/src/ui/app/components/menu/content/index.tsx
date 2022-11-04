// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { useCallback } from 'react';
import { Navigate, Route, Routes, useNavigate } from 'react-router-dom';

import Account from './account';
import MenuList from './menu-list';
import Network from './network';
import { ErrorBoundary } from '_components/error-boundary';
import {
    useMenuIsOpen,
    useMenuUrl,
    useNextMenuUrl,
} from '_components/menu/hooks';
import { useOnKeyboardEvent } from '_hooks';

import type { MouseEvent } from 'react';

import st from './MenuContent.module.scss';

const CLOSE_KEY_CODES: string[] = ['Escape'];

function MenuContent() {
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
    const expanded = menuUrl !== '/';
    if (!isOpen) {
        return null;
    }
    return (
        <div className={st.container}>
            <div className={st.backdrop} onClick={handleOnCloseMenu} />
            <div className={cl(st.content, { [st.expanded]: expanded })}>
                <ErrorBoundary>
                    <Routes location={menuUrl || ''}>
                        <Route path="/" element={<MenuList />} />
                        <Route path="/account" element={<Account />} />
                        <Route path="/network" element={<Network />} />
                        <Route
                            path="*"
                            element={
                                <Navigate to={menuHomeUrl} replace={true} />
                            }
                        />
                    </Routes>
                </ErrorBoundary>
            </div>
        </div>
    );
}

export default MenuContent;
