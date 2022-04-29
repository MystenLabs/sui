// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { MoveObjectType } from '../../src';

const PACKAGE = '0x2';
const MODULE_GENERIC = 'SUI';
const IDENTIFIER_GENERIC = 'SUI';
const GENERIC = `${PACKAGE}::${MODULE_GENERIC}::${IDENTIFIER_GENERIC}`;
const MODULE = 'Coin';
const IDENTIFIER = 'Coin';
const FULL_TYPE = `${PACKAGE}::${MODULE}::${IDENTIFIER}<${GENERIC}>`;

describe('Move Object Content Parsing', () => {
  it('parse MoveObjectType', async () => {
    const t = new MoveObjectType(FULL_TYPE);
    expect(t.getFullType()).toEqual(FULL_TYPE);
    expect(t.getPackageAddress()).toEqual(PACKAGE);
    expect(t.getModuleName()).toEqual(MODULE);
  });

  it('parse MoveObjectIdentifier', async () => {
    const identifier = new MoveObjectType(FULL_TYPE).getStructName();
    expect(identifier.getFullType()).toEqual(`${IDENTIFIER}<${GENERIC}>`);
    expect(identifier.getStructName()).toEqual(MODULE);
    expect(identifier.hasGenericType()).toEqual(true);
    const inner = identifier.getGenericType()!;
    expect(inner.getFullType()).toEqual(GENERIC);
    expect(inner.getPackageAddress()).toEqual(PACKAGE);
    expect(inner.getModuleName()).toEqual(MODULE_GENERIC);
    expect(inner.getStructName().hasGenericType()).toEqual(false);
    expect(inner.getStructName().getStructName()).toEqual(IDENTIFIER_GENERIC);
  });
});
