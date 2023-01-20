// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

export function Balance() {
  return (
    <div className="rounded-full shadow-notification bg-white px-4 py-1 flex items-center gap-11">
      <div>
        <div className="uppercase text-steel text-[10px] leading-tight font-semibold">
          In your wallet
        </div>
        <div className="text-steel-dark">
          <span className="font-semibold">1000</span> SUI
        </div>
      </div>
      <div className="rounded-full h-7 w-7 bg-sui"></div>
    </div>
  );
}
