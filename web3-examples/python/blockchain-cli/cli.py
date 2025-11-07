#!/usr/bin/env python3
"""
Blockchain CLI Tool
Command-line interface for blockchain operations
"""

import argparse
import sys
from web3 import Web3
from eth_account import Account
import json


class BlockchainCLI:
    """Command-line interface for blockchain operations"""

    def __init__(self, rpc_url: str):
        self.w3 = Web3(Web3.HTTPProvider(rpc_url))
        if not self.w3.is_connected():
            print(f"❌ Failed to connect to {rpc_url}")
            sys.exit(1)
        print(f"✅ Connected to blockchain (Chain ID: {self.w3.eth.chain_id})")

    def cmd_balance(self, address: str):
        """Get ETH balance"""
        balance_wei = self.w3.eth.get_balance(Web3.to_checksum_address(address))
        balance_eth = self.w3.from_wei(balance_wei, 'ether')
        print(f"\n💰 Balance: {balance_eth} ETH")
        print(f"   Address: {address}")

    def cmd_block(self, block_number: int):
        """Get block information"""
        if block_number < 0:
            block_number = self.w3.eth.block_number

        block = self.w3.eth.get_block(block_number)

        print(f"\n🔷 Block #{block['number']}")
        print(f"   Hash: {block['hash'].hex()}")
        print(f"   Timestamp: {block['timestamp']}")
        print(f"   Transactions: {len(block['transactions'])}")
        print(f"   Gas Used: {block['gasUsed']:,}")
        print(f"   Miner: {block['miner']}")

    def cmd_transaction(self, tx_hash: str):
        """Get transaction details"""
        tx = self.w3.eth.get_transaction(tx_hash)

        print(f"\n📜 Transaction: {tx_hash}")
        print(f"   From: {tx['from']}")
        print(f"   To: {tx['to']}")
        print(f"   Value: {self.w3.from_wei(tx['value'], 'ether')} ETH")
        print(f"   Gas Price: {self.w3.from_wei(tx['gasPrice'], 'gwei')} Gwei")
        print(f"   Block: {tx['blockNumber']}")

    def cmd_receipt(self, tx_hash: str):
        """Get transaction receipt"""
        receipt = self.w3.eth.get_transaction_receipt(tx_hash)

        status = "✅ Success" if receipt['status'] == 1 else "❌ Failed"
        print(f"\n📋 Transaction Receipt")
        print(f"   Status: {status}")
        print(f"   Block: {receipt['blockNumber']}")
        print(f"   Gas Used: {receipt['gasUsed']:,}")
        print(f"   Logs: {len(receipt['logs'])}")

    def cmd_create_wallet(self):
        """Create new wallet"""
        account = Account.create()

        print(f"\n🔐 New Wallet Created")
        print(f"   Address: {account.address}")
        print(f"   Private Key: {account.key.hex()}")
        print("\n⚠️  WARNING: Save your private key securely!")
        print("   Never share it or commit it to version control!")

    def cmd_gas_price(self):
        """Get current gas price"""
        gas_price_wei = self.w3.eth.gas_price
        gas_price_gwei = self.w3.from_wei(gas_price_wei, 'gwei')

        print(f"\n⛽ Current Gas Price")
        print(f"   {gas_price_gwei} Gwei")
        print(f"   {gas_price_wei:,} Wei")

    def cmd_network_info(self):
        """Display network information"""
        print(f"\n🌐 Network Information")
        print(f"   Chain ID: {self.w3.eth.chain_id}")
        print(f"   Latest Block: {self.w3.eth.block_number:,}")
        print(f"   Gas Price: {self.w3.from_wei(self.w3.eth.gas_price, 'gwei')} Gwei")
        print(f"   Syncing: {self.w3.eth.syncing}")


def main():
    parser = argparse.ArgumentParser(
        description='Blockchain CLI Tool',
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog='''
Examples:
  %(prog)s balance 0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045
  %(prog)s block 18000000
  %(prog)s tx 0x1234...
  %(prog)s create-wallet
  %(prog)s gas-price
  %(prog)s network-info
        '''
    )

    parser.add_argument(
        '--rpc',
        default='https://eth.llamarpc.com',
        help='RPC endpoint URL'
    )

    subparsers = parser.add_subparsers(dest='command', help='Commands')

    # Balance command
    parser_balance = subparsers.add_parser('balance', help='Get address balance')
    parser_balance.add_argument('address', help='Ethereum address')

    # Block command
    parser_block = subparsers.add_parser('block', help='Get block information')
    parser_block.add_argument('number', type=int, nargs='?', default=-1, help='Block number (-1 for latest)')

    # Transaction command
    parser_tx = subparsers.add_parser('tx', help='Get transaction details')
    parser_tx.add_argument('hash', help='Transaction hash')

    # Receipt command
    parser_receipt = subparsers.add_parser('receipt', help='Get transaction receipt')
    parser_receipt.add_argument('hash', help='Transaction hash')

    # Create wallet
    subparsers.add_parser('create-wallet', help='Create new wallet')

    # Gas price
    subparsers.add_parser('gas-price', help='Get current gas price')

    # Network info
    subparsers.add_parser('network-info', help='Display network information')

    args = parser.parse_args()

    if not args.command:
        parser.print_help()
        sys.exit(1)

    # Initialize CLI
    cli = BlockchainCLI(args.rpc)

    # Execute command
    if args.command == 'balance':
        cli.cmd_balance(args.address)
    elif args.command == 'block':
        cli.cmd_block(args.number)
    elif args.command == 'tx':
        cli.cmd_transaction(args.hash)
    elif args.command == 'receipt':
        cli.cmd_receipt(args.hash)
    elif args.command == 'create-wallet':
        cli.cmd_create_wallet()
    elif args.command == 'gas-price':
        cli.cmd_gas_price()
    elif args.command == 'network-info':
        cli.cmd_network_info()


if __name__ == '__main__':
    main()
