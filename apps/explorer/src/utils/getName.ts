// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// TODO : Import from  SDK
import { VALDIATOR_NAME } from '~/pages/validator/ValidatorDataTypes';

export function getName(rawName: string | number[]) {
    let name: string;

    if (Array.isArray(rawName)) {
        name = String.fromCharCode(...rawName);
    } else {
        name = decodeURIComponent(atob(rawName));
        if (!VALDIATOR_NAME.test(name)) {
            name = rawName;
        }
    }
    return name;
}
