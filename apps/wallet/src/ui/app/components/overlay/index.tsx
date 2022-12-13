// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { useCallback } from 'react';

import Icon, { SuiIcons } from '_components/icon';

import type { ReactNode } from 'react';

import st from './Overlay.module.scss';

type OverlayProps = {
    title: ReactNode;
    children: ReactNode | ReactNode[];
    showModal: boolean;
    closeOverlay?: () => void;
    closeIcon?: SuiIcons;
    setShowModal: (showModal: boolean) => void;
};

function Overlay({
    title,
    children,
    showModal,
    closeOverlay,
    setShowModal,
    closeIcon = SuiIcons.Close,
}: OverlayProps) {
    const closeModal = useCallback(
        (e: React.MouseEvent<HTMLElement>) => {
            closeOverlay && closeOverlay();
            setShowModal(false);
        },
        [closeOverlay, setShowModal]
    );

    return (
        <>
            {showModal ? (
                <div className={st.container}>
                    <div className={cl(st.header, 'bg-gray-40')}>
                        <div
                            className={cl(
                                st.headerContent,
                                'text-steel-darker'
                            )}
                        >
                            {title}
                        </div>
                    </div>
                    <div className={st.content}>{children}</div>
                    <button className={st.closeOverlay} onClick={closeModal}>
                        <Icon
                            icon={closeIcon}
                            className={cl(st.close, st[closeIcon])}
                        />
                    </button>
                </div>
            ) : null}
        </>
    );
}

export default Overlay;
