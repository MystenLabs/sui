// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// TODO : Import from  SDK
import { VALDIATOR_NAME } from '~/pages/validator/ValidatorDataTypes';

export function getName(rawName: string | number[]) {
    let name: string;
    if (Array.isArray(rawName)) return String.fromCharCode(...rawName);

    try {
        name = decodeURIComponent(atob(rawName));
        if (!VALDIATOR_NAME.test(name)) {
            name = rawName;
        }
    } catch (e) {
        name = rawName;
    }
    return name;
}
