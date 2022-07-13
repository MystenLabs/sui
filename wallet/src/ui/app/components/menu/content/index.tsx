// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useCallback } from 'react';
import { Navigate, Route, Routes, useNavigate } from 'react-router-dom';

import Settings from './settings';
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
    const menuHomeUrl = useNextMenuUrl(true, '/settings');
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
        <div className={st.container}>
            <div className={st.backdrop} onClick={handleOnCloseMenu} />
            <div className={st.content}>
                <Routes location={menuUrl || ''}>
                    <Route path="settings" element={<Settings />} />
                    <Route
                        path="*"
                        element={<Navigate to={menuHomeUrl} replace={true} />}
                    />
                </Routes>
            </div>
        </div>
    );
}

export default MenuContent;
