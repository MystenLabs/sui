// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Button } from '@mysten/ui';
import { type Meta, type StoryObj } from '@storybook/react';
import { useState } from 'react';

import { PlayPause, type PlayPauseProps } from '../PlayPause';

export default {
	component: PlayPause,
} as Meta;

export const Paused: StoryObj<PlayPauseProps> = {
	args: {
		paused: false,
	},
};

export const Unpaused: StoryObj<PlayPauseProps> = {
	args: {
		paused: true,
	},
};

export const Animated: StoryObj<PlayPauseProps> = {
	render: () => {
		const [startAnimation, setStartAnimation] = useState(false);
		const [animationDuration, setAnimationDuration] = useState(1);

		const setAnimationCallback = (seconds: number) => () => {
			setAnimationDuration(seconds);
			setStartAnimation(!startAnimation);
		};

		return (
			<div className="flex flex-col gap-2">
				<div className="flex gap-2">
					<Button onClick={setAnimationCallback(1)}>1 seconds</Button>
					<Button onClick={setAnimationCallback(5)}>5 seconds</Button>
					<Button onClick={setAnimationCallback(10)}>10 seconds</Button>
				</div>

				<div className="w-4">
					<PlayPause
						onChange={() => null}
						animate={{
							duration: animationDuration,
							start: startAnimation,
							setStart: setStartAnimation,
						}}
					/>
				</div>
			</div>
		);
	},
};
