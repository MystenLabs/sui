// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import useAppSelector from './useAppSelector';

export function useActiveAddress() {
    return useAppSelector(({ account }) => account.address);
}
