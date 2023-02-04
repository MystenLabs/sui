// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  LocalTxnDataSerializer,
  UnserializedSignableTransaction,
} from "@mysten/sui.js";
import { useWalletKit } from "@mysten/wallet-kit";
import { useMutation } from "@tanstack/react-query";
import { FormEvent, useEffect, useId } from "react";
import { useNavigate } from "react-router-dom";
import { Card } from "../components/Card";
import { config } from "../config";
import { useEpoch } from "../network/queries/epoch";
import { useScorecard } from "../network/queries/scorecard";
import { SUI_SYSTEM_ID } from "../network/queries/sui-system";
import provider from "../network/provider";

const GAS_BUDGET = 10000n;

export function Setup() {
  const id = useId();
  const navigate = useNavigate();
  const { currentAccount, signAndExecuteTransaction } = useWalletKit();
  const { data: scorecard, isSuccess } = useScorecard(currentAccount);
  const { data: epoch } = useEpoch();

  const createScorecard = useMutation(
    ["create-scorecard"],
    async (username: string) => {
      if (!currentAccount) {
        throw new Error("No SUI coins found in your wallet. You need SUI to play the Frenemies game");
      }

      const gasPrice = epoch?.data.referenceGasPrice || 1n;
      const checkTx: UnserializedSignableTransaction = {
        kind: "moveCall",
        data: {
          packageObjectId: config.VITE_PKG,
          module: "registry",
          function: "is_registered",
          typeArguments: [],
          arguments: [config.VITE_REGISTRY, username],
        },
      };

      const inspectRes = await provider.devInspectTransaction(
        currentAccount,
        checkTx,
        Number(gasPrice)
      );

      if ("Err" in inspectRes.results) {
        throw new Error(
          `Error happened while checking for uniqueness: ${inspectRes.results.Err}`
        );
      }

      const {
        Ok: [
          [
            ,
            {
              // @ts-ignore // not cool
              returnValues: [[[exists]]],
            },
          ],
        ],
      } = inspectRes.results;

      // Add a warning saying that the name is already taken.
      // Depending on the the `exists` result: 0 or 1;
      if (exists == 1) {
        throw new Error(`Name: '${username}' is already taken`);
      }

      const submitTx: UnserializedSignableTransaction = {
        kind: "moveCall",
        data: {
          packageObjectId: config.VITE_PKG,
          module: "frenemies",
          function: "register",
          arguments: [username, config.VITE_REGISTRY, SUI_SYSTEM_ID],
          typeArguments: [],

          // TODO: Fix in sui.js - add option to use bigint...
          gasBudget: Number(GAS_BUDGET),
        },
      };

      const serializer = new LocalTxnDataSerializer(provider);
      const serializedTx = await serializer.serializeToBytes(
        currentAccount,
        submitTx
      );
      const dryRunRes = await provider.dryRunTransaction(
        serializedTx.toString()
      );

      if (dryRunRes.status.status == "failure") {
        throw new Error(
          `Transaction would've failed with a reason '${dryRunRes.status.error}'`
        );
      }

      await signAndExecuteTransaction(submitTx);
    },
    {
      onSuccess() {
        navigate("/", { replace: true });
      },
    }
  );

  useEffect(() => {
    if (!currentAccount) {
      navigate("/connect", { replace: true });
    }
  }, [currentAccount]);

  useEffect(() => {
    if (isSuccess && scorecard) {
      navigate("/", { replace: true });
    }
  }, [scorecard, isSuccess]);

  const handleSubmit = (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    const formData = new FormData(e.currentTarget);
    createScorecard.mutate(formData.get("username") as string);
  };

  // TODO: Loading UI:
  if (!isSuccess || scorecard) {
    return null;
  }

  return (
    <div className="max-w-4xl w-full mx-auto text-center">
      <Card spacing="xl">
        <h1 className="text-steel-darker text-2xl leading-tight font-semibold mb-5">
          Woo hoo! Just one more step before we begin.
        </h1>
        <img src="/capy_singing.svg" className="mb-5 h-64 w-64 mx-auto" />
        <form onSubmit={handleSubmit}>
          <label
            htmlFor={id}
            className="text-steel-darker leading-tight mb-3 block"
          >
            What shall we call you in the game?
          </label>
          <input
            id={id}
            name="username"
            className="text-sm text-center w-56 mx-auto rounded-lg p-3 bg-white border border-gray-45 shadow-button leading-none"
            placeholder="Enter a player name"
            disabled={!isSuccess || createScorecard.isLoading}
            required
          />

          {/* TODO: Nice error handling, this is just for debugging: */}
          {createScorecard.isError && (
            <div className="mt-4 text-issue-dark text-left text-xs bg-issue-light rounded p-4">
              {String(createScorecard.error)}
            </div>
          )}

          <div className="mt-16">
            <button
              type="submit"
              className="shadow-notification bg-frenemies rounded-lg text-white disabled:text-white/50 px-5 py-3 w-56 leading-none"
              disabled={!isSuccess || createScorecard.isLoading}
            >
              Continue
            </button>
          </div>
        </form>
      </Card>
    </div>
  );
}
