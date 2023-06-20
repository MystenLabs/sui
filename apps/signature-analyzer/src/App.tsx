// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SignaturePubkeyPair, decodeMultiSig, toParsedSignaturePubkeyPair, publicKeyFromSerialized, SIGNATURE_SCHEME_TO_FLAG } from "@mysten/sui.js";
import { toB64 } from "@mysten/bcs";
import { useState } from "react";

function _appendBuffer(buffer1: Uint8Array, buffer2: Uint8Array) {
  var tmp = new Uint8Array(buffer1.byteLength + buffer2.byteLength);
  tmp.set(new Uint8Array(buffer1), 0);
  tmp.set(new Uint8Array(buffer2), buffer1.byteLength);
  var concat_array = new Uint8Array(tmp.buffer);
  return concat_array;
};

function _getSuiPubKey(signature: SignaturePubkeyPair): string {

  const key_flag = new Uint8Array(1);
  key_flag[0] = SIGNATURE_SCHEME_TO_FLAG[signature.signatureScheme];
  const flag_and_pk = _appendBuffer(key_flag, signature.pubKey.toBytes());
  const pubkey_base64_sui_format = toB64(flag_and_pk);
  return pubkey_base64_sui_format;

}

function Signature({ signature, index }: { signature: SignaturePubkeyPair, index: number }) {
  const suiPubkey = publicKeyFromSerialized(signature.signatureScheme, signature.pubKey.toString());
  const suiAddress = suiPubkey.toSuiAddress();

  const pubkey_base64_sui_format = _getSuiPubKey(signature);

  const pubkey = signature.pubKey.toBase64();
  const scheme = signature.signatureScheme.toString();

  return <div className="p-4 rounded bg-gray-100">
    <h3 className="text-2xl font-bold mb-2 bg-clip-text text-transparent bg-gradient-to-r from-indigo-500 to-pink-500">Signature #{index}</h3>
    <div className="mt-4">
      <div>
        <div className="text-lg font-bold mb-2">Signature Scheme</div>
        <code className="block bg-gray-200 p-2 text-gray-800 rounded text-sm break-all whitespace-pre-wrap">
          {scheme}
        </code>
      </div>
    </div>
    <div className="mt-4">
      <div>
        <div className="text-lg font-bold mb-2">Signature Public Key</div>
        <code className="block bg-gray-200 p-2 text-gray-800 rounded text-sm break-all whitespace-pre-wrap">
          {pubkey}
        </code>
      </div>
    </div>
    <div className="mt-4">
      <div>
        <div className="text-lg font-bold mb-2">Sui Format Public Key ( flag | pk )</div>
        <code className="block bg-gray-200 p-2 text-gray-800 rounded text-sm break-all whitespace-pre-wrap">
          {pubkey_base64_sui_format}
        </code>
      </div>
    </div>
    <div className="mt-4">
      <div>
        <div className="text-lg font-bold mb-2">Sui Address</div>
        <code className="block bg-gray-200 p-2 text-gray-800 rounded text-sm break-all whitespace-pre-wrap">
          {suiAddress}
        </code>
      </div>
    </div>
    <div className="mt-4">
      <div>
        <div className="text-lg font-bold mb-2">Signature</div>
        <code className="block bg-gray-200 p-2 text-gray-800 rounded text-sm break-all whitespace-pre-wrap">
          {toB64(signature.signature)}
        </code>
      </div>
    </div>
  </div>
}

export function App() {
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<Error | null>(null);
  const [listSignaturePubKeys, setListSignaturePubkeys] = useState<SignaturePubkeyPair[] | null>(null);

  return (
    <div className="m-8 px-8 py-6 mx-auto bg-white rounded-2xl shadow-md max-w-lg w-full">
      <div className="flex items-center justify-between mb-4">
        <h1 className="font-bold text-2xl">Signature Analyzer</h1>
      </div>

      {error && (
        <div className="bg-red-100 text-red-800 border border-red-300 px-2 py-1.5 rounded-md text-sm my-4">
          {error.message}
        </div>
      )}

      <div>
        <form
          onSubmit={async (e) => {
            e.preventDefault();
            setError(null);

            setLoading(true);
            try {
              const formData = new FormData(e.currentTarget);
              const signatureB64 = formData.get("bytes") as string;

              const signature = toParsedSignaturePubkeyPair(signatureB64);
              setListSignaturePubkeys(signature);

            } catch (e) {
              setError(e as Error);
            } finally {
              setLoading(false);
            }
          }}
        >
          <label
            htmlFor="bytes"
            className="block text-sm font-medium leading-6 text-gray-900"
          >
            Signature Bytes (base64 encoded)
          </label>
          <div className="mt-2">
            <textarea
              id="bytes"
              name="bytes"
              rows={3}
              className="block w-full rounded-md border-0 text-gray-900 shadow-sm ring-1 ring-inset ring-gray-300 placeholder:text-gray-400 focus:ring-2 focus:ring-inset focus:ring-indigo-600 sm:py-1.5 sm:text-sm sm:leading-6"
              defaultValue=""
            />
          </div>
          <div className="mt-2">
            <button
              type="submit"
              className="bg-indigo-600 text-sm font-medium text-white rounded-lg px-4 py-3 disabled:cursor-not-allowed disabled:opacity-60"
              disabled={loading}
            >
              Analyze Signature
            </button>
          </div>
        </form>

        <div className="flex flex-col gap-6 mt-8">
          {listSignaturePubKeys?.map((signature, index) => (
            <Signature index={index} signature={signature} />
          ))}
        </div>
      </div>
    </div>
  );
}


/* 
MultiSig (v1)
AwEAhhsJcCE+YgularrGwRj827fXQp52eVvrRBx3+cP67ZYJcT8W9Jc1FRBb05Aoaq3YJ6yQ/K/ZISFooxnyAuR1DxI6MAAAAQAAAAAAAAAQAAAAAAADLEFDYVk3VFcwTW5QdStmci9aMnFINVlSeWJIc2o4MHFmd2ZxaXVkdVQ0Y3ppASxBQnI4MThWWHQrNlBMUFJvQTdRbnNIQmZScEtKZFdaUGp0N3BwaVRsNkZrcQEsQUxERTNzcTVKWk9qM0htby9VZVV2MTR6aTRURlFNRnEveENUYVNIK3N3TVMBAQA=
*/

/* 
MultiSig (v2)
AwIAvlJnUP0iJFZL+QTxkKC9FHZGwCa5I4TITHS/QDQ12q1sYW6SMt2Yp3PSNzsAay0Fp2MPVohqyyA02UtdQ2RNAQGH0eLk4ifl9h1I8Uc+4QlRYfJC21dUbP8aFaaRqiM/f32TKKg/4PSsGf9lFTGwKsHJYIMkDoqKwI8Xqr+3apQzAwADAFriILSy9l6XfBLt5hV5/1FwtsIsAGFow3tefGGvAYCDAQECHRUjB8a3Kw7QQYsOcM2A5/UpW42G9XItP1IT+9I5TzYCADtqJ7zOtqQtYqOo0CpvDXNlMhV3HeJDpjrASKGLWdopAwMA
*/

/*
Single Sig
AIYbCXAhPmILpWq6xsEY/Nu310Kednlb60Qcd/nD+u2WCXE/FvSXNRUQW9OQKGqt2CeskPyv2SEhaKMZ8gLkdQ8mmO01tDJz7vn6/2dqh+WEcmx7I/NKn8H6ornbk+HM4g==
*/