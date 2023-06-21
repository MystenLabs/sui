// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { thunkExtras } from '../redux/store/thunk-extras';

export function useBackgroundClient() {
	return thunkExtras.background;
}
