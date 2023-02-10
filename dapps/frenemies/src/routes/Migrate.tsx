// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useWalletKit } from "@mysten/wallet-kit";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { useNavigate } from "react-router-dom";
import { Card } from "../components/Card";
import { Spinner } from "../components/Spinner";
import { config } from "../config";
import { useLegacyScorecard, useScorecard } from "../network/queries/scorecard";
import { SCORECARD } from "../network/types";

const GAS_BUDGET = 20000;

export function Migrate() {
  const { currentAccount, signAndExecuteTransaction } = useWalletKit();
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const scorecard = useScorecard();
  const legacyScorecard = useLegacyScorecard();

  useEffect(() => {
    if (!currentAccount) {
      navigate("/connect");
    }
  }, [currentAccount]);

  useEffect(() => {
    if (!scorecard.isSuccess || !legacyScorecard.isSuccess) return;

    if (scorecard.data) {
      navigate("/", { replace: true });
    }

    if (!scorecard.data && !legacyScorecard.data) {
      navigate("/setup", { replace: true });
    }
  }, [scorecard, legacyScorecard]);

  const migrateScorecard = useMutation(
    ["migrate-scorecard"],
    async () => {
      await signAndExecuteTransaction({
        kind: "moveCall",
        data: {
          packageObjectId: config.VITE_PKG,
          module: "registry",
          function: "is_registered",
          typeArguments: [],
          arguments: [config.VITE_REGISTRY],
          gasBudget: GAS_BUDGET,
        },
      });
    },
    {
      onSuccess() {
        queryClient.invalidateQueries({ queryKey: [SCORECARD] });
        navigate("/", { replace: true });
      },
    }
  );

  if (scorecard.isLoading || legacyScorecard.isLoading) {
    return <Spinner />;
  }

  return (
    <Card spacing="xl">
      <div className="space-y-5 text-center">
        <h1 className="text-steel-darker text-2xl leading-tight font-semibold mb-5">
          {scorecard.data
            ? `Welcome back, ${scorecard.data.data.name}!`
            : "Welcome back!"}
        </h1>
        <img src="/capy_singing.svg" className="mb-5 h-64 w-64 mx-auto" />
        <div className="text-steel-darker leading-tight mb-3 block">
          We need to update your scorecard before you can play.
        </div>
        <button
          type="button"
          onClick={() => migrateScorecard.mutate()}
          className="shadow-notification bg-frenemies rounded-lg text-white disabled:text-white/50 px-5 py-3 w-56 leading-none"
          disabled={migrateScorecard.isLoading}
        >
          Update Scorecard
        </button>
      </div>
    </Card>
  );
}
