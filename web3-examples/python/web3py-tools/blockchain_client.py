"""
Web3.py Blockchain Client
Comprehensive toolkit for Ethereum interactions using Python
"""

from web3 import Web3
from web3.middleware import geth_poa_middleware
from eth_account import Account
from typing import Optional, Dict, Any, List
import json


class BlockchainClient:
    """Main client for blockchain interactions"""

    def __init__(self, rpc_url: str, chain_id: Optional[int] = None):
        """
        Initialize blockchain client

        Args:
            rpc_url: RPC endpoint URL
            chain_id: Network chain ID
        """
        self.w3 = Web3(Web3.HTTPProvider(rpc_url))

        # Add PoA middleware for networks like BSC, Polygon
        self.w3.middleware_onion.inject(geth_poa_middleware, layer=0)

        if not self.w3.is_connected():
            raise ConnectionError(f"Failed to connect to {rpc_url}")

        self.chain_id = chain_id or self.w3.eth.chain_id
        print(f"✅ Connected to network (Chain ID: {self.chain_id})")

    def get_balance(self, address: str) -> float:
        """Get ETH balance of address"""
        checksum_address = Web3.to_checksum_address(address)
        balance_wei = self.w3.eth.get_balance(checksum_address)
        balance_eth = self.w3.from_wei(balance_wei, 'ether')
        return float(balance_eth)

    def get_block(self, block_number: int = -1) -> Dict[str, Any]:
        """Get block information"""
        if block_number == -1:
            block_number = self.w3.eth.block_number

        block = self.w3.eth.get_block(block_number)
        return dict(block)

    def get_transaction(self, tx_hash: str) -> Dict[str, Any]:
        """Get transaction details"""
        tx = self.w3.eth.get_transaction(tx_hash)
        return dict(tx)

    def get_transaction_receipt(self, tx_hash: str) -> Dict[str, Any]:
        """Get transaction receipt"""
        receipt = self.w3.eth.get_transaction_receipt(tx_hash)
        return dict(receipt)

    def send_transaction(
        self,
        private_key: str,
        to_address: str,
        value_eth: float,
        gas_price: Optional[int] = None
    ) -> str:
        """
        Send ETH transaction

        Returns:
            Transaction hash
        """
        account = Account.from_key(private_key)
        from_address = account.address

        # Build transaction
        tx = {
            'nonce': self.w3.eth.get_transaction_count(from_address),
            'to': Web3.to_checksum_address(to_address),
            'value': self.w3.to_wei(value_eth, 'ether'),
            'gas': 21000,
            'gasPrice': gas_price or self.w3.eth.gas_price,
            'chainId': self.chain_id,
        }

        # Sign transaction
        signed_tx = self.w3.eth.account.sign_transaction(tx, private_key)

        # Send transaction
        tx_hash = self.w3.eth.send_raw_transaction(signed_tx.rawTransaction)

        print(f"Transaction sent: {tx_hash.hex()}")
        return tx_hash.hex()

    def wait_for_transaction(self, tx_hash: str, timeout: int = 120) -> Dict[str, Any]:
        """Wait for transaction confirmation"""
        print(f"Waiting for transaction {tx_hash}...")
        receipt = self.w3.eth.wait_for_transaction_receipt(tx_hash, timeout=timeout)
        print(f"✅ Transaction confirmed in block {receipt['blockNumber']}")
        return dict(receipt)


class ERC20Client:
    """Client for ERC20 token interactions"""

    ABI = json.loads('''[
        {"constant":true,"inputs":[],"name":"name","outputs":[{"name":"","type":"string"}],"type":"function"},
        {"constant":true,"inputs":[],"name":"symbol","outputs":[{"name":"","type":"string"}],"type":"function"},
        {"constant":true,"inputs":[],"name":"decimals","outputs":[{"name":"","type":"uint8"}],"type":"function"},
        {"constant":true,"inputs":[],"name":"totalSupply","outputs":[{"name":"","type":"uint256"}],"type":"function"},
        {"constant":true,"inputs":[{"name":"account","type":"address"}],"name":"balanceOf","outputs":[{"name":"","type":"uint256"}],"type":"function"},
        {"constant":false,"inputs":[{"name":"to","type":"address"},{"name":"amount","type":"uint256"}],"name":"transfer","outputs":[{"name":"","type":"bool"}],"type":"function"},
        {"constant":true,"inputs":[{"name":"owner","type":"address"},{"name":"spender","type":"address"}],"name":"allowance","outputs":[{"name":"","type":"uint256"}],"type":"function"},
        {"constant":false,"inputs":[{"name":"spender","type":"address"},{"name":"amount","type":"uint256"}],"name":"approve","outputs":[{"name":"","type":"bool"}],"type":"function"}
    ]''')

    def __init__(self, w3: Web3, token_address: str):
        """Initialize ERC20 client"""
        self.w3 = w3
        self.contract = w3.eth.contract(
            address=Web3.to_checksum_address(token_address),
            abi=self.ABI
        )

    def get_info(self) -> Dict[str, Any]:
        """Get token information"""
        return {
            'name': self.contract.functions.name().call(),
            'symbol': self.contract.functions.symbol().call(),
            'decimals': self.contract.functions.decimals().call(),
            'total_supply': self.contract.functions.totalSupply().call(),
        }

    def balance_of(self, address: str) -> float:
        """Get token balance"""
        checksum_address = Web3.to_checksum_address(address)
        balance = self.contract.functions.balanceOf(checksum_address).call()
        decimals = self.contract.functions.decimals().call()
        return balance / (10 ** decimals)

    def transfer(
        self,
        private_key: str,
        to_address: str,
        amount: float
    ) -> str:
        """Transfer tokens"""
        account = Account.from_key(private_key)
        from_address = account.address

        decimals = self.contract.functions.decimals().call()
        amount_wei = int(amount * (10 ** decimals))

        # Build transaction
        tx = self.contract.functions.transfer(
            Web3.to_checksum_address(to_address),
            amount_wei
        ).build_transaction({
            'from': from_address,
            'nonce': self.w3.eth.get_transaction_count(from_address),
            'gas': 100000,
            'gasPrice': self.w3.eth.gas_price,
        })

        # Sign and send
        signed_tx = self.w3.eth.account.sign_transaction(tx, private_key)
        tx_hash = self.w3.eth.send_raw_transaction(signed_tx.rawTransaction)

        print(f"Token transfer sent: {tx_hash.hex()}")
        return tx_hash.hex()


def main():
    """Example usage"""

    # Connect to network
    client = BlockchainClient('https://eth.llamarpc.com')

    # Get Vitalik's balance
    vitalik = '0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045'
    balance = client.get_balance(vitalik)
    print(f"Vitalik's balance: {balance} ETH")

    # Get latest block
    block = client.get_block()
    print(f"Latest block: {block['number']}")
    print(f"Block hash: {block['hash'].hex()}")

    # Create account
    account = Account.create()
    print(f"\nNew account created:")
    print(f"Address: {account.address}")
    print(f"Private key: {account.key.hex()}")


if __name__ == '__main__':
    main()
