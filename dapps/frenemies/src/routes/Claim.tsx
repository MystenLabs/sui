import { ConnectButton } from "@mysten/wallet-kit";
import { useWalletKit } from "@mysten/wallet-kit";
import { Card } from "../components/Card";
import { Spinner } from "../components/Spinner";
import { config } from "../config";
import { useScorecard } from "../network/queries/scorecard";
import { useRawObject } from "../network/queries/use-raw";
import { LEADERBOARD, Leaderboard } from "../network/types";
import { useForm, useWatch } from "react-hook-form";
import { z } from "zod";
import { zodResolver } from "@hookform/resolvers/zod";
import { useMutation } from "@tanstack/react-query";
import { useSearchParams } from "react-router-dom";

function Connect() {
  return (
    <div>
      <img src="/capy_cowboy.svg" alt="Capy" className="block mx-auto" />
      <h1 className="mt-4 text-steel-darker text-heading2 font-semibold">
        Sui Frenemies game has ended.
      </h1>
      <div className="mt-4 text-steel-darker text-heading6">
        Connect your wallet to verify if you won.
      </div>
      <div className="mt-10">
        <ConnectButton className="!bg-frenemies !text-white !px-5 !py-3 leading-none" />
      </div>
    </div>
  );
}

function NoWinner() {
  return (
    <div>
      <img src="/capy_cry.svg" alt="Capy" className="block mx-auto" />
      <h1 className="mt-4 text-steel-darker text-heading2 leading-tight font-semibold">
        You haven't achieved a spot among the top 2000 players of the game.
      </h1>
      <div className="mt-4 text-steel-darker text-heading6 leading-tight">
        Thank you for helping test the Sui network by playing the Sui Frenemies
        game.
      </div>
    </div>
  );
}

function Done() {
  return (
    <div>
      <img src="/capy_thumbs_up.svg" alt="Capy" className="block mx-auto" />
      <h1 className="mt-4 text-steel-darker text-heading2 leading-tight font-semibold">
        Thank you!
      </h1>
      <div className="mt-4 text-steel-darker text-heading6 leading-tight">
        Your information has been submitted. We'll be in touch shortly.
      </div>
    </div>
  );
}

const Schema = z.object({
  name: z.string().min(1),
  email: z.string().email(),
  agreed: z.boolean(),
});

function Connected() {
  const { currentAccount, signAndExecuteTransaction } = useWalletKit();
  const [searchParams] = useSearchParams();
  const scorecard = useScorecard();
  const leaderboard = useRawObject<Leaderboard>(
    config.VITE_LEADERBOARD,
    LEADERBOARD
  );

  const { handleSubmit, register, formState, control } = useForm<
    z.infer<typeof Schema>
  >({
    resolver: zodResolver(Schema),
  });

  const agreed = useWatch({ name: "agreed", control });

  const submitWinner = useMutation(
    ["submit-winner"],
    async (values: z.infer<typeof Schema>) => {
      fetch(
        import.meta.env.DEV
          ? "http://127.0.0.1:3003/frenemies"
          : "https://apps-backend.sui.io/frenemies",
        {
          method: "POST",
          headers: {
            "content-type": "application/json",
          },
          body: JSON.stringify({
            address: currentAccount!,
            name: values.name,
            email: values.email,
          }),
        }
      );

      // await signAndExecuteTransaction();
    }
  );

  if (submitWinner.isSuccess) {
    return <Done />;
  }

  if (scorecard.isLoading || leaderboard.isLoading) {
    return (
      <div className="flex items-center justify-center -mt-32">
        <Spinner />
      </div>
    );
  }

  if (!scorecard.data || !leaderboard.data) {
    return <NoWinner />;
  }

  // TODO: Once we have a snapshot, use that instead of the live leaderboard.
  const rank = searchParams.get("rank")
    ? parseInt(searchParams.get("rank") as string, 10)
    : leaderboard.data.data.topScores.findIndex(
        (score) => score.name == scorecard.data!.data.name
      );

  if (rank === -1 || rank > 2000) {
    return <NoWinner />;
  }

  return (
    <div>
      <img src="/capy_cowboy.svg" alt="Capy" className="block mx-auto" />
      <h1 className="mt-4 text-steel-darker text-heading2 font-semibold leading-tight">
        You won. Congratulations!
      </h1>
      <div className="mt-4 text-steel-darker text-heading6 leading-normal">
        Please submit your name and email information below. The information
        submitted should match your exchange information.
        <br />
        <br />
        You will also be prompted to send a transaction to Testnet to validate
        ownership of your Sui address.
      </div>
      <form
        className="mt-10 flex flex-col gap-4"
        onSubmit={handleSubmit((values) => submitWinner.mutate(values))}
      >
        <input
          {...register("name")}
          className="text-sm w-full rounded-lg p-3 bg-white border border-gray-45 shadow-button leading-none"
          placeholder="Your name"
          type="text"
        />
        {formState.errors.name && (
          <div className="text-issue-dark bg-issue-light rounded text-sm text-left px-4 py-2 border border-issue">
            {formState.errors.name.message}
          </div>
        )}
        <input
          {...register("email")}
          className="text-sm w-full rounded-lg p-3 bg-white border border-gray-45 shadow-button leading-none"
          placeholder="Your email"
          type="email"
        />
        {formState.errors.email && (
          <div className="text-issue-dark bg-issue-light rounded text-sm text-left px-4 py-2 border border-issue">
            {formState.errors.email.message}
          </div>
        )}
        <label className="text-bodySmall text-gray-60 font-medium flex items-center justify-center">
          <input
            type="checkbox"
            className="h-4 w-4 rounded border-gray-60 text-frenemies focus:ring-frenemies mr-2"
            {...register("agreed")}
          />{" "}
          I read and agreed to the{" "}
          <a
            href="https://mystenlabs.com/legal?content=terms"
            target="_blank"
            rel="noopener noreferrer"
            className="text-sui-dark ml-1"
          >
            Terms of Service
          </a>
        </label>
        <button
          type="submit"
          disabled={!agreed || submitWinner.isLoading}
          className="bg-hero-darkest py-4 rounded-xl w-full text-white disabled:opacity-40 font-semibold"
        >
          Submit information
        </button>
      </form>
    </div>
  );
}

export function Claim() {
  const { currentAccount } = useWalletKit();

  return (
    <div className="max-w-xl mx-auto w-full text-center">
      <Card spacing="2xl">{currentAccount ? <Connected /> : <Connect />}</Card>
    </div>
  );
}
