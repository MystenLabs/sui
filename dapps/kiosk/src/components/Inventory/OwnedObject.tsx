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
          className="ease-in-out duration-300 rounded border border-transparent py-2 px-4 bg-gray-200"
          loading={mutation.isLoading}
          onClick={() => mutation.mutate({ fn: placeFn, object })}
        >
          Place in kiosk
        </Button>
        <Button
          loading={mutation.isLoading}
          className="ease-in-out duration-300 rounded py-2 btn-outline-primary"
          onClick={() => mutation.mutate({ fn: listFn, object })}
        >
          Sell in Kiosk
        </Button>
      </>
    </DisplayObject>
  );
}
