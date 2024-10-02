// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import Loading from '_components/loading';
import { useAppDispatch, useAppSelector } from '_hooks';
import { createDappStatusSelector } from '_redux/slices/permissions';
import { ampli } from '_src/shared/analytics/ampli';
import {
	arrow,
	offset,
	useClick,
	useDismiss,
	useFloating,
	useInteractions,
} from '@floating-ui/react';
import { ChevronDown12, Dot12 } from '@mysten/icons';
import { AnimatePresence, motion } from 'framer-motion';
import { memo, useCallback, useMemo, useRef, useState } from 'react';

import { useActiveAddress } from '../../hooks/useActiveAddress';
import { ButtonConnectedTo } from '../ButtonConnectedTo';
import { appDisconnect } from './actions';
import st from './DappStatus.module.scss';

function DappStatus() {
	const dispatch = useAppDispatch();
	const activeOriginUrl = useAppSelector(({ app }) => app.activeOrigin);
	const activeOrigin = useMemo(() => {
		try {
			return (activeOriginUrl && new URL(activeOriginUrl).hostname) || null;
		} catch (e) {
			return null;
		}
	}, [activeOriginUrl]);
	const activeOriginFavIcon = useAppSelector(({ app }) => app.activeOriginFavIcon);
	const activeAddress = useActiveAddress();
	const dappStatusSelector = useMemo(
		() => createDappStatusSelector(activeOriginUrl, activeAddress),
		[activeOriginUrl, activeAddress],
	);
	const isConnected = useAppSelector(dappStatusSelector);
	const [disconnecting, setDisconnecting] = useState(false);
	const [visible, setVisible] = useState(false);
	const onHandleClick = useCallback(
		(e: boolean) => {
			if (!disconnecting) {
				setVisible((isVisible) => !isVisible);
			}
		},
		[disconnecting],
	);
	const arrowRef = useRef(null);
	const {
		x,
		y,
		context,
		reference,
		refs,
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
		if (!disconnecting && isConnected && activeOriginUrl && activeAddress) {
			setDisconnecting(true);
			try {
				await dispatch(
					appDisconnect({
						origin: activeOriginUrl,
						accounts: [activeAddress],
					}),
				).unwrap();
				ampli.disconnectedApplication({
					applicationUrl: activeOriginUrl,
					disconnectedAccounts: 1,
					sourceFlow: 'Header',
				});
				setVisible(false);
			} catch (e) {
				// Do nothing
			} finally {
				setDisconnecting(false);
			}
		}
	}, [disconnecting, isConnected, activeOriginUrl, activeAddress, dispatch]);
	if (!isConnected) {
		return null;
	}
	return (
		<>
			<ButtonConnectedTo
				truncate
				iconBefore={<Dot12 className="text-success" />}
				text={activeOrigin || ''}
				iconAfter={<ChevronDown12 />}
				ref={reference}
				{...getReferenceProps()}
			/>
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
						ref={refs.setFloating}
					>
						<div className={st.popupContent}>
							<div className={st.originContainer}>
								{activeOriginFavIcon ? (
									<img src={activeOriginFavIcon} className={st.favicon} alt="App Icon" />
								) : null}
								<span className={st.originText}>
									<div>Connected to</div>
									<div className={st.originUrl}>{activeOrigin}</div>
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
						<div className={st.popupArrow} ref={arrowRef} style={{ left: arrowData?.x || 0 }} />
					</motion.div>
				) : null}
			</AnimatePresence>
		</>
	);
}

export default memo(DappStatus);
