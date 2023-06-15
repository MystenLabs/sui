// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Meta, type StoryObj } from '@storybook/react';

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
