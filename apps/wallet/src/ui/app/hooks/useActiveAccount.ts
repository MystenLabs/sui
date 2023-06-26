// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { activeAccountSelector } from '../redux/slices/account';
import useAppSelector from './useAppSelector';

export function useActiveAccount() {
	return useAppSelector(activeAccountSelector);
}
