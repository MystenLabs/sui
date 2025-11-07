import { useContractRead, useContractWrite, usePrepareContractWrite } from 'wagmi'
import { parseEther } from 'viem'
import { useState } from 'react'

// ERC20 ABI (simplified)
const ERC20_ABI = [
  {
    name: 'balanceOf',
    type: 'function',
    stateMutability: 'view',
    inputs: [{ name: 'account', type: 'address' }],
    outputs: [{ name: 'balance', type: 'uint256' }],
  },
  {
    name: 'transfer',
    type: 'function',
    stateMutability: 'nonpayable',
    inputs: [
      { name: 'to', type: 'address' },
      { name: 'amount', type: 'uint256' },
    ],
    outputs: [{ name: 'success', type: 'bool' }],
  },
] as const

interface ContractInteractionProps {
  contractAddress: `0x${string}`
  userAddress: `0x${string}`
}

/**
 * ERC20 Contract Interaction Component
 * Demonstrates reading and writing to smart contracts
 */
export function ERC20Interaction({
  contractAddress,
  userAddress
}: ContractInteractionProps) {
  const [recipient, setRecipient] = useState('')
  const [amount, setAmount] = useState('')

  // Read contract state
  const { data: balance, isLoading } = useContractRead({
    address: contractAddress,
    abi: ERC20_ABI,
    functionName: 'balanceOf',
    args: [userAddress],
    watch: true, // Poll for updates
  })

  // Prepare transaction
  const { config } = usePrepareContractWrite({
    address: contractAddress,
    abi: ERC20_ABI,
    functionName: 'transfer',
    args: recipient && amount ? [
      recipient as `0x${string}`,
      parseEther(amount)
    ] : undefined,
    enabled: Boolean(recipient && amount),
  })

  // Execute transaction
  const { write, isLoading: isTransferring } = useContractWrite(config)

  const handleTransfer = () => {
    if (write) {
      write()
      setRecipient('')
      setAmount('')
    }
  }

  return (
    <div className="contract-interaction">
      <div className="balance-section">
        <h3>Token Balance</h3>
        {isLoading ? (
          <p>Loading...</p>
        ) : (
          <p className="balance">{balance?.toString() || '0'} tokens</p>
        )}
      </div>

      <div className="transfer-section">
        <h3>Transfer Tokens</h3>

        <input
          type="text"
          placeholder="Recipient address (0x...)"
          value={recipient}
          onChange={(e) => setRecipient(e.target.value)}
          className="input-address"
        />

        <input
          type="text"
          placeholder="Amount"
          value={amount}
          onChange={(e) => setAmount(e.target.value)}
          className="input-amount"
        />

        <button
          onClick={handleTransfer}
          disabled={!write || isTransferring}
          className="btn-transfer"
        >
          {isTransferring ? 'Transferring...' : 'Transfer'}
        </button>
      </div>
    </div>
  )
}

/**
 * NFT Minter Component
 * Example of minting NFTs
 */
const NFT_ABI = [
  {
    name: 'mint',
    type: 'function',
    stateMutability: 'payable',
    inputs: [{ name: 'tokenURI', type: 'string' }],
    outputs: [{ name: 'tokenId', type: 'uint256' }],
  },
] as const

export function NFTMinter({ nftContractAddress }: { nftContractAddress: `0x${string}` }) {
  const [tokenURI, setTokenURI] = useState('')

  const { config } = usePrepareContractWrite({
    address: nftContractAddress,
    abi: NFT_ABI,
    functionName: 'mint',
    args: tokenURI ? [tokenURI] : undefined,
    value: parseEther('0.05'), // Mint price
    enabled: Boolean(tokenURI),
  })

  const { write: mint, isLoading: isMinting } = useContractWrite(config)

  return (
    <div className="nft-minter">
      <h3>Mint NFT</h3>

      <input
        type="text"
        placeholder="Token URI (ipfs://...)"
        value={tokenURI}
        onChange={(e) => setTokenURI(e.target.value)}
        className="input-uri"
      />

      <button
        onClick={() => mint?.()}
        disabled={!mint || isMinting}
        className="btn-mint"
      >
        {isMinting ? 'Minting...' : 'Mint NFT (0.05 ETH)'}
      </button>
    </div>
  )
}
