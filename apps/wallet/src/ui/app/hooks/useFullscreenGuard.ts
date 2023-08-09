// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect } from 'react';

import useAppSelector from './useAppSelector';
import { AppType } from '_redux/slices/app/AppType';
import { openInNewTab } from '_shared/utils';

export default function useFullscreenGuard(enabled: boolean) {
	const appType = useAppSelector((state) => state.app.appType);
	useEffect(() => {
		if (enabled && appType === AppType.popup) {
			openInNewTab().finally(() => window.close());
		}
	}, [appType, enabled]);
	return !enabled && appType === AppType.unknown;
}
