import { useAccount, useConnect, useDisconnect, useBalance, useNetwork } from 'wagmi'
import { InjectedConnector } from 'wagmi/connectors/injected'
import { formatEther } from 'viem'

/**
 * WalletConnect Component
 * Demonstrates Wagmi hooks for Web3 wallet integration
 */
export function WalletConnect() {
  const { address, isConnected } = useAccount()
  const { connect } = useConnect({
    connector: new InjectedConnector(),
  })
  const { disconnect } = useDisconnect()
  const { data: balance } = useBalance({
    address: address,
  })
  const { chain } = useNetwork()

  if (isConnected && address) {
    return (
      <div className="wallet-connected">
        <div className="wallet-info">
          <h3>Connected Wallet</h3>
          <p className="address">
            {address.slice(0, 6)}...{address.slice(-4)}
          </p>

          <div className="balance">
            <span className="label">Balance:</span>
            <span className="value">
              {balance ? formatEther(balance.value) : '0'} {balance?.symbol}
            </span>
          </div>

          <div className="network">
            <span className="label">Network:</span>
            <span className="value">{chain?.name || 'Unknown'}</span>
          </div>
        </div>

        <button
          onClick={() => disconnect()}
          className="btn-disconnect"
        >
          Disconnect Wallet
        </button>
      </div>
    )
  }

  return (
    <div className="wallet-disconnected">
      <h3>Connect Your Wallet</h3>
      <p>Connect your Web3 wallet to interact with the DApp</p>
      <button
        onClick={() => connect()}
        className="btn-connect"
      >
        Connect Wallet
      </button>
    </div>
  )
}

/**
 * Token Balance Component
 * Display ERC20 token balance
 */
export function TokenBalance({ tokenAddress }: { tokenAddress: `0x${string}` }) {
  const { address } = useAccount()
  const { data: tokenBalance } = useBalance({
    address: address,
    token: tokenAddress,
  })

  if (!tokenBalance) return null

  return (
    <div className="token-balance">
      <span className="token-symbol">{tokenBalance.symbol}</span>
      <span className="token-amount">
        {formatEther(tokenBalance.value)}
      </span>
    </div>
  )
}
