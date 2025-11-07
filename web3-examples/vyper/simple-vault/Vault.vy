# @version ^0.3.7
"""
@title Simple Vault
@author Web3 Multi-Language Playground
@notice A basic vault contract for depositing and withdrawing ETH
"""

# Events
event Deposit:
    sender: indexed(address)
    amount: uint256
    balance: uint256

event Withdraw:
    receiver: indexed(address)
    amount: uint256
    balance: uint256

# Storage
balances: public(HashMap[address, uint256])
total_supply: public(uint256)
owner: public(address)

@external
def __init__():
    """
    @notice Contract constructor
    """
    self.owner = msg.sender

@external
@payable
def deposit():
    """
    @notice Deposit ETH into the vault
    """
    assert msg.value > 0, "Must deposit positive amount"

    self.balances[msg.sender] += msg.value
    self.total_supply += msg.value

    log Deposit(msg.sender, msg.value, self.balances[msg.sender])

@external
def withdraw(amount: uint256):
    """
    @notice Withdraw ETH from the vault
    @param amount The amount to withdraw
    """
    assert amount > 0, "Must withdraw positive amount"
    assert self.balances[msg.sender] >= amount, "Insufficient balance"

    self.balances[msg.sender] -= amount
    self.total_supply -= amount

    send(msg.sender, amount)

    log Withdraw(msg.sender, amount, self.balances[msg.sender])

@external
def withdraw_all():
    """
    @notice Withdraw all deposited ETH
    """
    balance: uint256 = self.balances[msg.sender]
    assert balance > 0, "No balance to withdraw"

    self.balances[msg.sender] = 0
    self.total_supply -= balance

    send(msg.sender, balance)

    log Withdraw(msg.sender, balance, 0)

@view
@external
def get_balance(account: address) -> uint256:
    """
    @notice Get balance of an account
    @param account The account address
    @return The balance of the account
    """
    return self.balances[account]

@view
@external
def get_total_supply() -> uint256:
    """
    @notice Get total ETH in vault
    @return Total supply
    """
    return self.total_supply
