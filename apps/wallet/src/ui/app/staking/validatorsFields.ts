// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    is,
    SuiObject,
    type ValidatorsFields,
    type GetObjectDataResponse,
} from '@mysten/sui.js';

export function validatorsFields(
    data?: GetObjectDataResponse
): ValidatorsFields | null {
    return data &&
        is(data.details, SuiObject) &&
        data.details.data.dataType === 'moveObject'
        ? (data.details.data.fields as ValidatorsFields)
        : null;
}
