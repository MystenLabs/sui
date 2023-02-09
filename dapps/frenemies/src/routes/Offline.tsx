import { Card } from "../components/Card";

export function Offline() {
  return (
    <div className="absolute top-1/2 left-1/2 max-w-4xl w-full -translate-x-1/2 -translate-y-1/2 px-4">
      <Card spacing="xl" variant="error">
        <div className="flex flex-col md:flex-row gap-16 items-center">
          <img src="/capy_cry.svg" alt="Sad Capy" className="flex-1" />
          <div className="text-heading2 leading-tight">
            <div className="font-bold">
              Frenemies is currently offline while we upgrade the app to provide
              players with the best experience.
            </div>
            <div className="mt-4">
              We expect Frenemies to be available again by Friday, February 10
              at 12 p.m. PST.
            </div>
          </div>
        </div>
      </Card>
    </div>
  );
}
