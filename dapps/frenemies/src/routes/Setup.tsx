// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { UnserializedSignableTransaction } from "@mysten/sui.js";
import { useWalletKit } from "@mysten/wallet-kit";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { FormEvent, useEffect, useId } from "react";
import { useNavigate } from "react-router-dom";
import { Card } from "../components/Card";
import { config } from "../config";
import { useLegacyScorecard, useScorecard } from "../network/queries/scorecard";
import { SUI_SYSTEM_ID } from "../network/queries/sui-system";
import provider from "../network/provider";
import { useBalance } from "../network/queries/coin";
import { Spinner } from "../components/Spinner";

const GAS_BUDGET = 20000n;

export function Setup() {
  const id = useId();
  const navigate = useNavigate();
  const { currentAccount, signAndExecuteTransaction } = useWalletKit();
  const { data: scorecard, isSuccess } = useScorecard();
  const legacyScorecard = useLegacyScorecard();
  const { data: balance } = useBalance();
  const { data: gasPrice } = useQuery(
    ["gas-price"],
    () => provider.getReferenceGasPrice(),
    {
      refetchInterval: false,
      refetchOnWindowFocus: false,
    }
  );
  const queryClient = useQueryClient();

  useEffect(() => {
    if (
      isSuccess &&
      legacyScorecard.isSuccess &&
      !scorecard &&
      legacyScorecard.data
    ) {
      navigate("/migrate", { replace: true });
    }
  }, [isSuccess, scorecard, legacyScorecard]);

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

  const createScorecard = useMutation(
    ["create-scorecard"],
    async (username: string) => {
      if (!currentAccount) {
        throw new Error("No connected wallet found");
      }

      const inspectResults = await Promise.all(
        [config.VITE_OLD_REGISTRY, config.VITE_REGISTRY].map((registry) =>
          provider.devInspectTransaction(
            currentAccount,
            {
              kind: "moveCall",
              data: {
                packageObjectId: config.VITE_PKG,
                module: "registry",
                function: "is_registered",
                typeArguments: [],
                arguments: [registry, username],
              },
            },
            gasPrice
          )
        )
      );

      inspectResults.forEach(({ results }) => {
        if ("Err" in results) {
          throw new Error(
            `Error happened while checking for uniqueness: ${results.Err}`
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
        } = results;

        // Add a warning saying that the name is already taken.
        // Depending on the the `exists` result: 0 or 1;
        if (exists == 1) {
          throw new Error(`Name: '${username}' is already taken`);
        }
      });

      await signAndExecuteTransaction({
        kind: "moveCall",
        data: {
          packageObjectId: config.VITE_PKG,
          module: "frenemies",
          function: "register",
          arguments: [
            username,
            config.VITE_REGISTRY,
            config.VITE_OLD_REGISTRY,
            SUI_SYSTEM_ID,
          ],
          typeArguments: [],

          // TODO: Fix in sui.js - add option to use bigint...
          gasBudget: Number(GAS_BUDGET),
        },
      });
    },
    {
      onSuccess() {
        queryClient.invalidateQueries({ queryKey: ["scorecard"] });
        navigate("/", { replace: true });
      },
    }
  );

  const handleSubmit = (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    const formData = new FormData(e.currentTarget);
    createScorecard.mutate(formData.get("username") as string);
  };

  const hasEnoughCoins =
    gasPrice && balance
      ? balance.balance > BigInt(gasPrice) + GAS_BUDGET
      : false;

  if (!isSuccess || scorecard || legacyScorecard.isLoading) {
    return <Spinner />;
  }

  return (
    <div className="max-w-4xl w-full mx-auto text-center space-y-5">
      {balance && gasPrice && !hasEnoughCoins && (
        <Card variant="error" spacing="md">
          Your wallet does not have enough SUI to register for Frenemies.
          <a
            className="font-medium text-issue block mt-1"
            href="https://discord.com/channels/916379725201563759/1037811694564560966"
            target="_blank"
            rel="noopener noreferrer"
          >
            Request Testnet SUI on Discord
          </a>
        </Card>
      )}

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
              disabled={
                !hasEnoughCoins || !isSuccess || createScorecard.isLoading
              }
            >
              Continue
            </button>
          </div>
        </form>
      </Card>
    </div>
  );
}
