// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    is,
    SuiObject,
    type MoveSuiSystemObjectFields,
    type GetObjectDataResponse,
} from '@mysten/sui.js';

export function validatorsFields(
    data?: GetObjectDataResponse
): MoveSuiSystemObjectFields | null {
    return data &&
        is(data.details, SuiObject) &&
        data.details.data.dataType === 'moveObject'
        ? (data.details.data.fields as MoveSuiSystemObjectFields)
        : null;
}
