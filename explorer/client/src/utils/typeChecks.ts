// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

export const checkIsIDType = (key: string, value: any) =>
    /owned/.test(key) ||
    (/_id/.test(key) && value?.bytes) ||
    value?.vec ||
    key === 'objects';

export const hasBytesField = (value: any) => value?.bytes;
export const hasVecField = (value: any) => value?.vec;
export const checkVecOfSingleID = (value: any) =>
    Array.isArray(value) && value.length > 0 && value[0]?.bytes;

export const isSuiPropertyType = (value: any) =>
    ['number', 'string'].includes(typeof value);