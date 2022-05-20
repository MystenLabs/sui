// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { createContext, type Dispatch, type SetStateAction } from 'react';

import { type Network } from './utils/api/DefaultRpcClient';

export const NetworkContext = createContext<
    [Network | string, Dispatch<SetStateAction<Network | string>>]
>(['', () => null]);
