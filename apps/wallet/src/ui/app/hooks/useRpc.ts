// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { api } from '../redux/store/thunk-extras';

export function useRpc() {
    return api.instance.fullNode;
}
