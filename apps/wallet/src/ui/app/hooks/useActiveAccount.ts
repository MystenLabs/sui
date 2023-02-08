// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import useAppSelector from './useAppSelector';

export function useActiveAccount() {
    return useAppSelector(({ account }) => account.account);
}
