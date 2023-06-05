import { SuiConnectButton } from './SuiConnectButton';

export function WalletNotConnected(): JSX.Element {
  return (
    <div className=" mb-12 flex items-center justify-center">
      <div className="flex justify-center min-h-[70vh] items-center">
        <div className="text-center">
          <div>
            <h2 className="font-bold text-2xl">
              Connect your wallet to manage your kiosk
            </h2>
            <p className="pb-6 pt-3">
              Create your kiosk to manage your kiosk and <br />
              purchase from other kiosks.
            </p>
          </div>
          <SuiConnectButton />
        </div>
      </div>
    </div>
  );
}
