import { useWalletKit, type WalletWithFeatures } from "@mysten/wallet-kit";

type CustodialConnectInput = {};
type SuiWalletCustodialConnectFeature = {
  "suiWallet:custodialConnect": {
    version: "0.0.1";
    custodialConnect: (input: CustodialConnectInput) => Promise<void>;
  };
};
type CustodialConnectWallet = WalletWithFeatures<
  Partial<SuiWalletCustodialConnectFeature>
>;

export function CustodialConnectButton() {
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
  const custodialConnectWallet =
    selectedWallet.wallet as CustodialConnectWallet;
  return (
    <button
      onClick={async () => {
        try {
          custodialConnectWallet.features[
            "suiWallet:custodialConnect"
          ]?.custodialConnect({});
        } catch (e) {
          console.log(e);
        }
      }}
    >
      Connect
    </button>
  );
}
