// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromB64 } from '@mysten/sui.js';
import { useQuery } from '@tanstack/react-query';

import { useRpc } from '../hooks';

export const VALDIATOR_NAME = /^[A-Z-_.\s0-9]+$/i;

const textDecoder = new TextDecoder();

export function getName(rawName: string | number[]) {
    let name: string;

    if (Array.isArray(rawName)) {
        name = String.fromCharCode(...rawName);
    } else {
        name = textDecoder.decode(fromB64(rawName));
        if (!VALDIATOR_NAME.test(name)) {
            name = rawName;
        }
    }
    return name;
}

export function useSystemState() {
    const rpc = useRpc();
    return useQuery(['system', 'state'], () => rpc.getSuiSystemState());
}
