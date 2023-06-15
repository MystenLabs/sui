// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { AutorefreshPause24, AutorefreshPlay24 } from '@mysten/icons';

import { IconButton } from './IconButton';

export interface PlayPauseProps {
	paused?: boolean;
	onChange(): void;
}

// TODO: Have this leverage the `IconButton` component:
export function PlayPause({ paused, onChange }: PlayPauseProps) {
	return (
		<IconButton
			aria-label={paused ? 'Paused' : 'Playing'}
			icon={paused ? AutorefreshPlay24 : AutorefreshPause24}
			onClick={onChange}
			className="cursor-pointer border-none bg-transparent text-steel hover:text-steel-darker"
		/>
	);
}
