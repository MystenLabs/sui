// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { createContext } from 'react';

import { ReplayType } from './replay-types';

export type ReplayContextProps = {
	network: string;
	data: ReplayType | null;
};
export const ReplayContext = createContext<ReplayContextProps | null>(null);
