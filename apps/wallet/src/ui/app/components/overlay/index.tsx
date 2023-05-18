// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { X32 } from '@mysten/icons';
import cl from 'classnames';
import { useCallback } from 'react';

import useAppSelector from '../../hooks/useAppSelector';
import { AppType } from '../../redux/slices/app/AppType';
import { Portal } from '../../shared/Portal';

import type { ReactNode } from 'react';

import st from './Overlay.module.scss';

type OverlayProps = {
    title?: ReactNode;
    children: ReactNode;
    showModal: boolean;
    closeOverlay?: () => void;
    closeIcon?: ReactNode | null;
    setShowModal?: (showModal: boolean) => void;
};

function Overlay({
    title,
    children,
    showModal,
    closeOverlay,
    setShowModal,
    closeIcon = <X32 fill="currentColor" className="text-sui-light w-8 h-8" />,
}: OverlayProps) {
    const closeModal = useCallback(
        (e: React.MouseEvent<HTMLElement>) => {
            closeOverlay && closeOverlay();
            setShowModal && setShowModal(false);
        },
        [closeOverlay, setShowModal]
    );
    const appType = useAppSelector((state) => state.app.appType);
    const isFullScreen = appType === AppType.fullscreen;

    return showModal ? (
        <Portal containerId="overlay-portal-container">
            <div
                className={cl(st.container, {
                    [st.fullScreenContainer]: isFullScreen,
                })}
            >
                {title && (
                    <div className="bg-gray-40 h-12 w-full">
                        <div className="text-steel-darker bg-gray-40 flex justify-center h-12 items-center text-heading4 font-semibold">
                            {title}
                        </div>
                    </div>
                )}
                <div
                    className={st.content}
                    style={{
                        height: title
                            ? 'calc(100% - 128px)'
                            : 'calc(100% - 80px)',
                    }}
                >
                    {children}
                </div>
                <button className={st.closeOverlay} onClick={closeModal}>
                    {closeIcon}
                </button>
            </div>
        </Portal>
    ) : null;
}

export default Overlay;
