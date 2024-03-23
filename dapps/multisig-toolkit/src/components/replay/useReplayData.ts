// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useContext } from 'react';

import { ReplayContext } from './ReplayContext';

/**
 * An easy helper to get the replay data from the context.
 * It's only callable from components that are children of the ReplayProvider.
 * This is defined in `/routes/replay.tsx`.
 */
export function useReplayData() {
	const replayContext = useContext(ReplayContext);
	if (!replayContext)
		throw new Error('Replay context not found. Please make sure ReplayContext is set.');

	return replayContext;
}
