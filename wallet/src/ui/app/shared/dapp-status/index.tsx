// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cn from 'classnames';
import { memo, useCallback, useMemo, useRef, useState } from 'react';
import { usePopper } from 'react-popper';

import { appDisconnect } from './actions';
import Icon, { SuiIcons } from '_components/icon';
import Loading from '_components/loading';
import { useAppDispatch, useAppSelector, useOnClickOutside } from '_hooks';
import { createDappStatusSelector } from '_redux/slices/permissions';

import st from './DappStatus.module.scss';

function DappStatus() {
    const dispatch = useAppDispatch();
    const activeOriginUrl = useAppSelector(({ app }) => app.activeOrigin);
    const activeOrigin = useMemo(
        () => (activeOriginUrl && new URL(activeOriginUrl).hostname) || null,
        [activeOriginUrl]
    );
    const activeOriginFavIcon = useAppSelector(
        ({ app }) => app.activeOriginFavIcon
    );
    const dappStatusSelector = useMemo(
        () => createDappStatusSelector(activeOriginUrl),
        [activeOriginUrl]
    );
    const isConnected = useAppSelector(dappStatusSelector);
    const [disconnecting, setDisconnecting] = useState(false);
    const [visible, setVisible] = useState(false);
    const Component = isConnected ? 'button' : 'span';
    const [referenceElement, setReferenceElement] =
        useState<HTMLButtonElement | null>(null);
    const [popperElement, setPopperElement] = useState<HTMLDivElement | null>(
        null
    );
    const [arrowElement, setArrowElement] = useState<HTMLDivElement | null>(
        null
    );
    const { styles, attributes } = usePopper(referenceElement, popperElement, {
        placement: 'bottom',
        modifiers: [
            { name: 'arrow', options: { element: arrowElement } },
            { name: 'offset', options: { offset: [0, 10] } },
        ],
    });
    const onHandleClick = useCallback(() => {
        if (!disconnecting) {
            setVisible((isVisible) => !isVisible);
        }
    }, [disconnecting]);
    const onHandleClickOutside = useCallback(
        (e: Event) => {
            if (visible) {
                e.stopPropagation();
                e.preventDefault();
                e.stopImmediatePropagation();
                if (!disconnecting) {
                    setVisible(false);
                }
            }
        },
        [visible, disconnecting]
    );
    const wrapperRef = useRef(null);
    useOnClickOutside(wrapperRef, onHandleClickOutside);
    const onHandleDisconnect = useCallback(async () => {
        if (!disconnecting && isConnected && activeOriginUrl) {
            setDisconnecting(true);
            await dispatch(appDisconnect({ origin: activeOriginUrl })).unwrap();
            setVisible(false);
            setDisconnecting(false);
        }
    }, [disconnecting, isConnected, activeOriginUrl, dispatch]);
    if (!isConnected) {
        return null;
    }
    return (
        <div className={st.wrapper} ref={wrapperRef}>
            <Component
                type="button"
                className={cn(st.container, {
                    [st.connected]: isConnected,
                    [st.active]: visible,
                })}
                ref={setReferenceElement}
                disabled={!isConnected}
                onClick={isConnected ? onHandleClick : undefined}
            >
                <Icon
                    icon="circle-fill"
                    className={cn(st.icon, { [st.connected]: isConnected })}
                />
                <span className={st.label}>
                    {(isConnected && activeOrigin) || 'Not connected'}
                </span>
                {isConnected ? (
                    <Icon icon={SuiIcons.ChevronDown} className={st.chevron} />
                ) : null}
            </Component>
            {visible && isConnected ? (
                <div
                    className={st.popup}
                    ref={setPopperElement}
                    style={styles.popper}
                    {...attributes.popper}
                >
                    <div className={st.popupContent}>
                        <div className={st.originContainer}>
                            {activeOriginFavIcon ? (
                                <img
                                    src={activeOriginFavIcon}
                                    className={st.favicon}
                                    alt="App Icon"
                                />
                            ) : null}
                            <span className={st.originText}>
                                <div>Connected to</div>
                                <div className={st.originUrl}>
                                    {activeOrigin}
                                </div>
                            </span>
                        </div>
                        <div className={st.divider} />
                        <Loading loading={disconnecting}>
                            <button
                                type="button"
                                className={st.disconnect}
                                onClick={onHandleDisconnect}
                                disabled={disconnecting}
                            >
                                Disconnect App
                            </button>
                        </Loading>
                    </div>
                    <div
                        className={st.popupArrow}
                        ref={setArrowElement}
                        style={styles.arrow}
                    />
                </div>
            ) : null}
        </div>
    );
}

export default memo(DappStatus);
