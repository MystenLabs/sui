// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useWalletKit } from "@mysten/wallet-kit";
import { useMutation } from "@tanstack/react-query";
import { FormEvent, useEffect, useId } from "react";
import { useNavigate } from "react-router-dom";
import { Card } from "../components/Card";
import { config } from "../config";
import { useScorecard } from "../network/queries/scorecard";
import { SUI_SYSTEM_ID } from "../network/queries/sui-system";

export function Setup() {
  const id = useId();
  const navigate = useNavigate();
  const { currentAccount, signAndExecuteTransaction } = useWalletKit();
  const { data: scorecard, isSuccess } = useScorecard(currentAccount);

  const createScorecard = useMutation(
    ["create-scorecard"],
    async (username: string) => {
      await signAndExecuteTransaction({
        kind: "moveCall",
        data: {
          packageObjectId: config.VITE_PKG,
          module: "frenemies",
          function: "register",
          arguments: [username, config.VITE_REGISTRY, SUI_SYSTEM_ID],
          typeArguments: [],
          gasBudget: 10000,
        },
      });
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
