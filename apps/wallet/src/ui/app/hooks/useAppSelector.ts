// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { RootState } from '_redux/RootReducer';
import { useSelector } from 'react-redux';
import type { TypedUseSelectorHook } from 'react-redux';

const useAppSelector: TypedUseSelectorHook<RootState> = useSelector;

export default useAppSelector;
