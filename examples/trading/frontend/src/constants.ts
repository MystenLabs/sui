// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// You can choose a different env (e.g. using a .env file, or a predefined list)
import demoContract from "../../api/demo-contract.json";
import escrowContract from "../../api/escrow-contract.json";

export enum QueryKey {
  Locked = "locked",
  Escrow = "escrow",
  GetOwnedObjects = "getOwnedObjects",
}

export const CONSTANTS = {
  escrowContract: {
    ...escrowContract,
    lockedType: `${escrowContract.packageId}::lock::Locked`,
    lockedKeyType: `${escrowContract.packageId}::lock::Key`,
    lockedObjectDFKey: `${escrowContract.packageId}::lock::LockedObjectKey`,
  },
  demoContract: {
    ...demoContract,
    demoBearType: `${demoContract.packageId}::demo_bear::DemoBear`,
  },
  apiEndpoint: "http://localhost:3000/",
};
