// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { AppType } from '_redux/slices/app/AppType';
import { openInNewTab } from '_shared/utils';
import { useEffect, useRef } from 'react';

import useAppSelector from './useAppSelector';

export default function useFullscreenGuard(enabled: boolean) {
	const appType = useAppSelector((state) => state.app.appType);
	const isOpenTabInProgressRef = useRef(false);
	useEffect(() => {
		if (enabled && appType === AppType.popup && !isOpenTabInProgressRef.current) {
			isOpenTabInProgressRef.current = true;
			openInNewTab().finally(() => window.close());
		}
	}, [appType, enabled]);
	return !enabled && appType === AppType.unknown;
}
