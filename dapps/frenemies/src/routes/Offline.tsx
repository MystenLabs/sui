import { Card } from "../components/Card";

export function Offline() {
  return (
    <div className="absolute top-1/2 left-1/2 max-w-2xl w-full -translate-x-1/2 -translate-y-1/2 px-4">
      <Card spacing="xl" variant="error">
        <div className="flex flex-col md:flex-row gap-16 items-center">
          <img src="/capy_cry.svg" alt="Sad Capy" className="flex-1" />
          <div className="text-heading2">
            <div className="font-bold">
              We're aware of issues and are working to fix them.
            </div>
            <div className="mt-4">Please check back soon.</div>
          </div>
        </div>
      </Card>
    </div>
  );
}
