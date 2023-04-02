import { useWalletKit, type WalletWithFeatures } from "@mysten/wallet-kit";

type QredoConnectInput = {};
type QredoConnectFeature = {
  "qredo:connect": {
    version: "0.0.1";
    qredoConnect: (input: QredoConnectInput) => Promise<void>;
  };
};
type QredoConnectWallet = WalletWithFeatures<Partial<QredoConnectFeature>>;

export function QredoConnectButton() {
  const { wallets } = useWalletKit();
  // just select the first one, it will probably be SUI until any other wallets implement this feature.
  // Then a select wallet modal might be useful.
  const selectedWallet = wallets[0];
  if (!selectedWallet || !("wallet" in selectedWallet)) {
    return (
      <a
        href="https://chrome.google.com/webstore/detail/sui-wallet/opcgpfmipidbgpenhmajoajpbobppdil"
        target="_blank"
      >
        Install Sui Wallet to stake SUI
      </a>
    );
  }
  const qredoConnectWallet = selectedWallet.wallet as QredoConnectWallet;
  return (
    <button
      onClick={async () => {
        try {
          qredoConnectWallet.features["qredo:connect"]?.qredoConnect({});
        } catch (e) {
          console.log(e);
        }
      }}
    >
      Connect
    </button>
  );
}
