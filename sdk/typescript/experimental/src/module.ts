// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SuiObjectRef, CallArg, TypeTag, MoveCallTx } from "./types";

/**
 * Base class for helping implement MoveCall functionality.
 * Wraps the module package reference and a name and makes sure transactions
 * follow the format required by the gateway.
 *
 * @example
 * class DevNetNFT extends sui.MoveModule {
 *   constructor(public pkg: SuiObjectRef) { super(pkg, 'devnet_nft'); }
 *
 *   // Move function is:
 *   // public entry fun mint(name: vector<u8>, description: vector<u8>, url: vector<u8>)
 *   public mint(name: string, description: string, url: string): MoveCallTx {
 *     return this.call('mint', [], [
 *       { Pure: bcs.ser(bcs.STRING, name).toBytes() },
 *       { Pure: bcs.ser(bcs.STRING, description).toBytes() },
 *       { Pure: bcs.ser(bcs.STRING, url).toBytes() }
 *     ]);
 *   }
 * }
 */
export class MoveModule {
  constructor(public pkg: SuiObjectRef, public name: string) {}

  /**
   * Build a transaction from passed in arguments.
   * Makes sure that the resulting format is BCS-able.
   */
  public call(
    method: string,
    type_args: TypeTag[],
    args: CallArg[]
  ): MoveCallTx {
    return {
      Call: {
        package: this.pkg,
        module: this.name,
        function: method,
        typeArguments: type_args,
        arguments: args,
      },
    };
  }
}
