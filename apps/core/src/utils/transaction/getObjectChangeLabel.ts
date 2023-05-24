// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SuiObjectChangeTypes } from './types';

export const ObjectChangeLabels = {
    created: 'Create',
    mutated: 'Update',
    transferred: 'Transfer',
    published: 'Publish',
    deleted: 'Delete',
    wrapped: 'Wrap',
};

export function getObjectChangeLabel(type: SuiObjectChangeTypes) {
    return ObjectChangeLabels[type];
}
