// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { KioskData } from '../Kiosk/KioskData';
import { OwnedObjectType } from './OwnedObjects';
import { DisplayObject } from '../DisplayObject';
import { useState } from 'react';
import { actionWithLoader } from '../../utils/buttons';
import { Button } from '../Base/Button';

export function OwnedObject({
  object,
  placeFn,
  listFn,
}: KioskData & {
  listFn: (item: OwnedObjectType) => void;
  placeFn: (item: OwnedObjectType) => void;
  object: OwnedObjectType;
}): JSX.Element {
  const [loading, setLoading] = useState<boolean>(false);

  return (
    <DisplayObject item={object}>
      <>
        <Button
          loading={loading}
          onClick={() => actionWithLoader(placeFn, object, setLoading)}
        >
          Place in kiosk
        </Button>
        <Button
          loading={loading}
          onClick={() => actionWithLoader(listFn, object, setLoading)}
          className="btn-outline-primary"
        >
          Sell in Kiosk
        </Button>
      </>
    </DisplayObject>
  );
}
