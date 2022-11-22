// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ReactComponent as PauseIcon } from './icons/pause.svg';
import { ReactComponent as PlayIcon } from './icons/play.svg';

export interface PlayPauseProps {
    paused?: boolean;
    onChange(): void;
}

// TODO: Create generalized `IconButton` component:
export function PlayPause({ paused, onChange }: PlayPauseProps) {
    return (
        <button
            type="button"
            aria-label={paused ? 'Paused' : 'Playing'}
            onClick={onChange}
            className="border-none bg-transparent cursor-pointer text-steel hover:text-steel-darker"
        >
            {paused ? <PlayIcon /> : <PauseIcon />}
        </button>
    );
}
