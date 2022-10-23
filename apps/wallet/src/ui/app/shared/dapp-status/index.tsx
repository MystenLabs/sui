// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    useFloating,
    useInteractions,
    useClick,
    useDismiss,
    offset,
    arrow,
} from '@floating-ui/react-dom-interactions';
import cn from 'classnames';
import { motion, AnimatePresence } from 'framer-motion';
import { memo, useCallback, useMemo, useRef, useState } from 'react';

import { appDisconnect } from './actions';
import Icon, { SuiIcons } from '_components/icon';
import Loading from '_components/loading';
import { useAppDispatch, useAppSelector } from '_hooks';
import { createDappStatusSelector } from '_redux/slices/permissions';
import { trackEvent } from '_src/shared/plausible';

import st from './DappStatus.module.scss';

function DappStatus() {
    const dispatch = useAppDispatch();
    const activeOriginUrl = useAppSelector(({ app }) => app.activeOrigin);
    const activeOrigin = useMemo(() => {
        try {
            return (
                (activeOriginUrl && new URL(activeOriginUrl).hostname) || null
            );
        } catch (e) {
            return null;
        }
    }, [activeOriginUrl]);
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
    const onHandleClick = useCallback(
        (e: boolean) => {
            if (!disconnecting) {
                setVisible((isVisible) => !isVisible);
            }
        },
        [disconnecting]
    );
    const arrowRef = useRef(null);
    const {
        x,
        y,
        context,
        reference,
        floating,
        middlewareData: { arrow: arrowData },
    } = useFloating({
        open: visible,
        onOpenChange: onHandleClick,
        placement: 'bottom',
        middleware: [offset(8), arrow({ element: arrowRef })],
    });
    const { getFloatingProps, getReferenceProps } = useInteractions([
        useClick(context),
        useDismiss(context, {
            outsidePressEvent: 'click',
            bubbles: false,
        }),
    ]);
    const onHandleDisconnect = useCallback(async () => {
        if (!disconnecting && isConnected && activeOriginUrl) {
            trackEvent('AppDisconnect', {
                props: { source: 'Header' },
            });
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
        <>
            <Component
                type="button"
                className={cn(st.container, {
                    [st.connected]: isConnected,
                    [st.active]: visible,
                })}
                disabled={!isConnected}
                ref={reference}
                {...getReferenceProps()}
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
            <AnimatePresence>
                {visible ? (
                    <motion.div
                        initial={{
                            opacity: 0,
                            scale: 0,
                            y: 'calc(-50% - 15px)',
                        }}
                        animate={{ opacity: 1, scale: 1, y: 0 }}
                        exit={{ opacity: 0, scale: 0, y: 'calc(-50% - 15px)' }}
                        transition={{
                            duration: 0.3,
                            ease: 'anticipate',
                        }}
                        className={st.popup}
                        style={{ top: y || 0, left: x || 0 }}
                        {...getFloatingProps()}
                        ref={floating}
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
                            ref={arrowRef}
                            style={{ left: arrowData?.x || 0 }}
                        />
                    </motion.div>
                ) : null}
            </AnimatePresence>
        </>
    );
}

export default memo(DappStatus);
