// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { filter, lastValueFrom, map, race, Subject, take } from 'rxjs';
import { v4 as uuidV4 } from 'uuid';
import Browser from 'webextension-polyfill';

import { Window } from './Window';

import type { ContentScriptConnection } from '_src/background/connections/ContentScriptConnection';
import type {
  SignatureRequest,
  SignatureRequestResponse
} from '_src/shared/messaging/messages/payloads/signatures';

const SIG_STORE_KEY = 'signatures';

function openTxWindow(sigRequestId: string) {
  return new Window(
    Browser.runtime.getURL('ui.html') +
    `#/dapp/sign-message/${encodeURIComponent(sigRequestId)}`
  );
}

class Signatures {
  private _sigResponseMessages = new Subject<SignatureRequestResponse>();

  public async signMessage(
    message: Uint8Array,
    connection: ContentScriptConnection
  ) {
    const sigRequest = this.createSignatureRequest(
      message,
      connection.origin,
      connection.originFavIcon
    );
    await this.storeSignatureRequest(sigRequest);
    const popUp = openTxWindow(sigRequest.id);
    const popUpClose = (await popUp.show()).pipe(
      take(1),
      map<number, false>(() => false)
    );
    const sigResponseMessage = this._sigResponseMessages.pipe(
      filter((msg) => msg.sigId === sigRequest.id),
      take(1)
    );
    return lastValueFrom(
      race(popUpClose, sigResponseMessage).pipe(
        take(1),
        map(async (response) => {
          if (response) {
            const { signed, sigResult, sigResultError } = response;
            if (signed) {
              sigRequest.signed = signed;
              sigRequest.sigResult = sigResult;
              sigRequest.sigResultError = sigResultError;
              await this.storeSignatureRequest(sigRequest);
              if (sigResultError) {
                throw new Error(
                  `Signature failed with the following error. ${sigResultError}`
                );
              }
              if (!sigResult) {
                throw new Error(`Signature result is empty`);
              }
              return sigResult;
            }
          }
          await this.removeSignatureRequest(sigRequest.id);
          throw new Error('Signature rejected from user');
        })
      )
    );
  }

  public async getSignatureRequests(): Promise<
    Record<string, SignatureRequest>
  > {
    return (await Browser.storage.local.get({ [SIG_STORE_KEY]: {} }))[
      SIG_STORE_KEY
    ];
  }

  public async getSignatureRequest(
    sigRequestId: string
  ): Promise<SignatureRequest | null> {
    return (await this.getSignatureRequests())[sigRequestId] || null;
  }

  public handleMessage(msg: SignatureRequestResponse) {
    this._sigResponseMessages.next(msg);
  }

  private createSignatureRequest(
    message: Uint8Array,
    origin: string,
    originFavIcon?: string
  ): SignatureRequest {
    return {
      id: uuidV4(),
      signed: null,
      origin,
      originFavIcon,
      createdDate: new Date().toISOString(),
      message,
    };
  }

  private async saveSignatureRequests(
    sigRequests: Record<string, SignatureRequest>
  ) {
    await Browser.storage.local.set({ [SIG_STORE_KEY]: sigRequests });
  }

  private async storeSignatureRequest(sigRequest: SignatureRequest) {
    const signatures = await this.getSignatureRequests();
    signatures[sigRequest.id] = sigRequest;
    await this.saveSignatureRequests(signatures);
  }

  private async removeSignatureRequest(sigId: string) {
    const signatures = await this.getSignatureRequests();
    delete signatures[sigId];
    await this.saveSignatureRequests(signatures);
  }
}

export default new Signatures();
