// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// TODO - handle multiple date formats
// Wed Aug 05
export default function useFormatDate(date: Date) {
    return new Date(date).toLocaleDateString('en-us', {
        weekday: 'short',
        month: 'short',
        day: 'numeric',
    });
}
