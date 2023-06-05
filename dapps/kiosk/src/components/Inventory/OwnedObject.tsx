// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { OwnedObjectType } from './OwnedObjects';
import { DisplayObject } from '../DisplayObject';
import { Button } from '../Base/Button';
import { KioskFnType, useKioskMutationFn } from '../../hooks/kiosk';

export function OwnedObject({
  object,
  placeFn,
  listFn,
}: {
  listFn: KioskFnType;
  placeFn: KioskFnType;
  object: OwnedObjectType;
}): JSX.Element {
  const mutation = useKioskMutationFn();

  return (
    <DisplayObject item={object}>
      <>
        <Button
          loading={mutation.isLoading}
          onClick={() => mutation.mutate({ fn: placeFn, object })}
        >
          Place in kiosk
        </Button>
        <Button
          loading={mutation.isLoading}
          onClick={() => mutation.mutate({ fn: listFn, object })}
          className="btn-outline-primary"
        >
          Sell in Kiosk
        </Button>
      </>
    </DisplayObject>
  );
}
