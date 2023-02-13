import { useEffect } from "react";
import { useNavigate } from "react-router-dom";
import { GAME_END_DATE, gameIsOver } from "../config";
import { Card } from "./Card";

export function useGameOverRedirect() {
  const navigate = useNavigate();

  useEffect(() => {
    if (gameIsOver()) {
      navigate("/claim", { replace: true });
      return;
    }

    const timer = setTimeout(() => {
      navigate("/claim", { replace: true });
    }, GAME_END_DATE.getTime() - Date.now());
    return () => {
      clearTimeout(timer);
    };
  }, []);
}

export function GameEnding() {
  return (
    <Card>
      <h2 className="text-steel-darker font-semibold text-heading2">
        Frenemies is ending.
      </h2>
      <div className="text-left text-steel-darker mt-4">
        The game will end on{" "}
        <span className="font-bold">
          {GAME_END_DATE.toLocaleString("en-US", {
            day: "numeric",
            month: "long",
            minute: "2-digit",
            hour: "numeric",
            timeZoneName: "short",
          })}
        </span>
        . Please make sure you submit your scores beforehand.
      </div>
    </Card>
  );
}
