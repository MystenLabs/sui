// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useCreateKioskMutation } from '../../mutations/kiosk';
import { Button } from '../Base/Button';

export function KioskCreation({ onCreate }: { onCreate: () => void }) {
  const createKiosk = useCreateKioskMutation({
    onSuccess: onCreate,
  });

  return (
    <div className="min-h-[70vh] flex items-center justify-center gap-4 mt-6 text-center">
      <div>
        <h2 className="font-bold text-2xl">You don't have a kiosk yet.</h2>
        <p>Create your kiosk to start trading.</p>
        <Button
          loading={createKiosk.isLoading}
          onClick={() => createKiosk.mutate()}
          className="mt-8 bg-primary text-white"
        >
          Create your Kiosk
        </Button>
      </div>
    </div>
  );
}
