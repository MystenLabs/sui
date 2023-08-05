#!/usr/bin/env python3

from enum import IntEnum


################################################################
# Object model for transactions and gas analysis
#

class MoveCalls:
    """
    Move call info.
    Packages, modules and functions are tracked, together with the number of calls for each.
    Natives are tracked with their overall charge per native
    So each of the maps is from a package/module/function to the number of calls to that "element"
    E.g:
    p1::m1::f1()
    p1::m1::f2()
    p2::m2::g1()
    p2::m2::g1()
    will lead to the following 3 maps
    pacakges:
    p1 => 2
    p2 => 2
    modules:
    p1::m1 => 2
    p2::m2 => 2
    functions:
    p1::m1::f1 => 1
    p1::m1::f2 => 1
    p2::m2::g1 => 2
    """

    def __init__(self):
        # package => call_count
        self.packages = {}
        # module => call_count
        self.modules = {}
        # function => call_count
        self.functions = {}

    def __str__(self):
        move_call = "MoveCall(functions:[ "
        for function in self.functions.items():
            move_call = f"{move_call}{function[0]}({function[1]}) "
        return f"{move_call}])"

    def packages(self):
        """Return the packages called in this programmable transaction"""
        return self.packages.keys()

    def modules(self):
        """Return the modules called in this programmable transaction"""
        return self.modules.keys()

    def functions(self):
        """Return the functions called in this programmable transaction"""
        return self.functions.keys()

    def call_count(self):
        """Return the total number of calls in this programmable transaction"""
        return sum(self.functions.values())

    def does_call_package(self, package):
        """Return true if the programmable transaction calls the given package"""
        return package in self.packages

    def does_calls_module(self, module):
        """Return true if the programmable transaction calls the given module"""
        return module in self.modules

    def does_calls_function(self, function):
        """Return true if the programmable transaction calls the given function"""
        return function in self.functions


class ProgrammableTransaction:
    """
    ProgrammableTransaction info.
    Tracks the number of each command per programmable transaction and some extra
    data for packages and move calls
    """

    def __init__(self):
        # pair of (number of publish commands, bytes of publishing commands)
        self.publish = (0, 0)
        # TODO: track same info as publish, for now just number of upgrades
        self.upgrade = 0
        self.transfer = 0
        self.split = 0
        self.merge = 0
        self.make_move_vec = 0
        self.gas_info = None
        self.move_calls = MoveCalls()

    def __str__(self):
        return f"ProgrammableTransaction(" \
               f"publish[{self.publish}], " \
               f"upgrade[{self.upgrade}], " \
               f"transfer[{self.transfer}], " \
               f"split[{self.split}], " \
               f"merge[{self.merge}], " \
               f"make_move_vec[{self.make_move_vec}], " \
               f"gas_info[{self.gas_info}], " \
               f"move_calls[{self.move_calls}])"

    def is_valid(self):
        return self.gas_info is not None

    def command_count(self):
        """Return the total number of commands in this programmable transaction"""
        return (
                self.publish[0] + self.upgrade + self.transfer +
                self.split + self.merge + self.make_move_vec +
                self.move_calls.call_count()
        )

    def move_call_count(self):
        """Return the total number of move calls in this programmable transaction"""
        return self.move_calls.call_count()

    def simple_command_count(self):
        """Return the total number of non move calls or publish/upgrade commands in this programmable transaction"""
        return self.transfer + self.split + self.merge + self.make_move_vec

    def publish_command_count(self):
        """Return the total number of publish or upgrade commands in this programmable transaction"""
        return self.publish[0] + self.upgrade


class GasInfo:
    """
    Gas information.
    Report "external" (GasSummary) and internal (GasStatus) info.
    """

    # computation cost is normalized to a `gas_price = 1000`.
    # Across networks and epochs the gas price can vary and thus the computation cost can vary.
    # To normalize the computation cost we divide it by the gas price and multiply it by 1000
    gas_cost_normalizer = 1000

    def __init__(self):
        self.budget = 0
        self.gas_price = 0
        self.computation_cost = 0
        self.computation_cost_rounded = 0
        self.gas_used = 0
        self.storage_cost = 0
        self.storage_rebate = 0
        self.non_refundable_storage_fee = 0
        self.instruction_count = 0
        self.stack_height = 0
        self.stack_size = 0

    def __str__(self):
        return f"GasInfo(" \
               f"budget[{self.budget}], " \
               f"gas_price[{self.gas_price}], " \
               f"computation_cost[{self.computation_cost}], " \
               f"computation_cost_rounded[{self.computation_cost_rounded}], " \
               f"gas_used[{self.gas_used}], " \
               f"storage_cost[{self.storage_cost}], " \
               f"storage_rebate[{self.storage_rebate}]), " \
               f"non_refundable_storage_fee[{self.non_refundable_storage_fee}], " \
               f"instruction_count[{self.instruction_count}], " \
               f"stack_height[{self.stack_height}], " \
               f"stack_size[{self.stack_size}])"


    def add_computation_cost(self, cost):
        self.computation_cost = cost / self.gas_price * self.gas_cost_normailizer

    def add_computation_cost_rounded(self, cost):
        self.computation_cost_rounded = cost / self.gas_price * self.gas_cost_normailizer


class TransactionType(IntEnum):
    """Transaction types."""
    UNDEFINED = 0
    GENESIS = 1
    CONSENSUS_COMMIT_PROLOGUE = 2
    CHANGE_EPOCH = 3
    PROGRAMMABLE_TRANSACTION = 4


class ExecutionResult(IntEnum):
    """
    Execution results broken down by results of interest.
    Not all results are tracked so this is not about results in transaction effects.
    """
    SUCCESS = 0
    INSUFFICIENT_GAS = 1
    INVARIANT_VIOLATION = 2
    GENERIC = 3


class Transaction:
    """
    Transaction data and representation.
    Holds transaction effects and more private information about programmable transactions and gas.
    Move natives are stored here because system transactions can call move natives.
    """

    def __init__(self):
        self.txn_type = TransactionType.UNDEFINED
        self.digest = None
        self.signer = None
        self.times = []
        self.error = ExecutionResult.SUCCESS
        self.shared_objects = 0
        self.created = 0
        self.mutated = 0
        self.unwrapped = 0
        self.deleted = 0
        self.unwrapped_deleted = 0
        self.wrapped = 0
        self.programmable_transaction = None
        self.natives = {}

    def __str__(self):
        natives = "natives:["
        for native in self.natives.items():
            natives = f"{natives} {native[0]}{native[1]} "
        natives = f"{natives}]"
        return f"Transaction(" \
               f"txn_type[{self.txn_type.name}], " \
               f"digest[{self.digest}], " \
               f"signer[{self.signer}], " \
               f"times[{self.times}], " \
               f"error[{self.error.name}], " \
               f"shared_objects[{self.shared_objects}], " \
               f"created[{self.created}], " \
               f"mutated[{self.mutated}], " \
               f"unwrapped[{self.unwrapped}], " \
               f"deleted[{self.deleted}], " \
               f"unwrapped_deleted[{self.unwrapped_deleted}], " \
               f"wrapped[{self.wrapped}], " \
               f"programmable_transaction[{self.programmable_transaction}], " \
               f"{natives})"

    def is_valid(self):
        if self.txn_type == TransactionType.UNDEFINED or not self.times:
            return False
        elif self.txn_type == TransactionType.PROGRAMMABLE_TRANSACTION:
            return self.programmable_transaction.is_valid()
        return True

    def successful(self):
        """Return true if the transaction was successful."""
        return self.is_valid() and self.error == ExecutionResult.SUCCESS

    def is_programmable(self):
        return self.is_valid() and self.txn_type == TransactionType.PROGRAMMABLE_TRANSACTION

    def is_system(self):
        return self.is_valid() and self.txn_type != TransactionType.PROGRAMMABLE_TRANSACTION

    def objects_touched(self):
        return (
                self.shared_objects + self.created + self.mutated
                + self.unwrapped + self.deleted + self.unwrapped_deleted
                + self.wrapped
        )

################################################################
# Helpers for transactions manipulation.
#

def instructions(txns):
    """
    Return a list of (instruction count, digest) for each transaction
    in the set of programmable transactions in input.
    """
    return [(txn.programmable_transaction.gas_info.instruction_count, txn.digest) for txn in txns]


def stack_height(txns):
    """
    Return a list of (stack height, digest) for each transaction
    in the set of programmable transactions in input
    """
    return [(txn.programmable_transaction.gas_info.stack_height, txn.digest) for txn in txns]


def mem_size(txns):
    """
    Return a list of (stack size, digest) for each transaction
    in the set of programmable transactions in input
    """
    return [(txn.programmable_transaction.gas_info.stack_size, txn.digest) for txn in txns]


def execution_results(txns):
    """
    Return a breakdown of the execution results for the set of transactions in input.
    Returned tuple: (successful, out_of_gas_err, inv_violations_err, generic_err)
    """
    successful = 0
    out_of_gas_err = 0
    inv_violations_err = 0
    generic_err = 0
    for txn in txns:
        if txn.error == ExecutionResult.SUCCESS:
            successful += 1
        elif txn.error == ExecutionResult.INSUFFICIENT_GAS:
            out_of_gas_err += 1
        elif txn.error == ExecutionResult.INVARIANT_VIOLATION:
            inv_violations_err += 1
        elif txn.error == ExecutionResult.GENERIC:
            generic_err += 1
    (successful, out_of_gas_err, inv_violations_err, generic_err)


def programmable_break_down(programmable, publish_txns, move_txns, simple_txns):
    """
    Break down programmable transactions in input into publish/upgrade, move and simple.
    When a transaction contains multiple commands the priority is publish/upgrade, then move, then simple.
    In other words, if a transaction has a single publish/upgrade command and other commands it  will be
    classified as publish/upgrade and added to publish_txns. If a transaction has a single move command, it will
    classified as move and added to move_txns even if it has non move commands.
    """
    for txn in programmable:
        prog = txn.programmable_transaction
        if prog.publish[0] > 0 or prog.upgrade > 0:
            publish_txns.append(txn)
        elif prog.move_calls.move_call_count() > 0:
            move_txns.append(txn)
        else:
            simple_txns.append(txn)


def collect_times(txns, times):
    """
    Load times for a set of transactions in a given list.
    Each transaction may have multiple times and this function flattens them.
    Return a list of (time, digest) for each time of each transaction in input.
    """
    for txn in txns:
        for t in txn.times:
            times.append((t, txn.digest))


def gas_break_down(txns, txn_info):
    """
    Break down of main gas components for a set of transactions in a given list.
    Return a list of (digest, time, gas used, computation cost, instructions, stack height, stack size)
    for each transaction in the set of programmable transactions in input.
    """
    for txn in txns:
        digest = txn.digest
        pt = txn.programmable_transaction
        gas = pt.gas_info
        computation = gas.computation_cost
        gas_used = gas.gas_used
        instr = gas.instruction_count
        height = gas.stack_height
        size = gas.stack_size
        for t in txn.times:
            txn_info.append((digest, t, gas_used, computation, instr, height, size))


def commands_break_down(txns, cmds):
    """
    Given a list of programmable transactions, break them down by the number of commands overall,
    simple commands, publish commands and move calls. Collect also time and digest for each element in the list
    """
    for txn in txns:
        prog_txn = txn.programmable_transaction
        publish = prog_txn.publish_command_count()
        simple = prog_txn.simple_command_count()
        move_calls = prog_txn.move_calls.move_call_count()
        cmds.append((publish + simple + move_calls, simple, move_calls, publish, txn.times, txn.digest))


################################################################
# DataFrame all things

import pandas as pd

def create_dataframe(normalized_transaction_list):
    """
    Create a dataframe from a list of normalized transactions.
    See `normalize_transactions` for the format of the normalized transactions.
    """
    df = pd.DataFrame(
        normalized_transaction_list,
        columns=[
            # time taken to execute the transaction
            "time",
            # basic transaction info
            "digest", "signer", "txn_type", "result",
            # object usage
            "shared", "created", "mutated", "unwrapped", "deleted", "unwrapped_deleted", "wrapped",
            # native calls
            "native_call_count", "natives",
            # gas info
            "computation_cost", "computation_cost_rounded", "gas_used", "storage_cost", "storage_rebate",
            "non_refundable_storage_fee", "instruction_count", "stack_height", "stack_size",
            # commands
            "publish", "upgrade", "transfer", "split", "merge", "make_move_vec",
            # move calls
            "move_call_count", "packages", "modules", "functions"
        ]
    )
    return df

def normalize_transactions(txns):
    """
    Normalize a list of transactions as defined in the object model into a flat list of tuples.
    The resulting list of lists is later used to create a dataframe.
    """
    normalized_list = []
    for txn in txns:
        normalized_txn = []
        # basic transaction info
        normalized_txn.append(txn.digest)
        normalized_txn.append(txn.signer)
        normalized_txn.append(txn.txn_type.value)
        normalized_txn.append(txn.error.value)
        # object usage
        normalized_txn.append(txn.shared_objects)
        normalized_txn.append(txn.created)
        normalized_txn.append(txn.mutated)
        normalized_txn.append(txn.unwrapped)
        normalized_txn.append(txn.deleted)
        normalized_txn.append(txn.unwrapped_deleted)
        normalized_txn.append(txn.wrapped)
        # natives
        natives_count = 0
        for native in txn.natives.values():
            natives_count += native[0]
        normalized_txn.append(natives_count)
        normalized_txn.append(txn.natives)

        # gas
        computation_cost = 0
        computation_cost_rounded = 0
        gas_used = 0
        storage_cost = 0
        storage_rebate = 0
        non_refundable_storage_fee = 0
        instruction_count = 0
        stack_height = 0
        stack_size = 0
        # commands
        publish = 0
        upgrade = 0
        transfer = 0
        split = 0
        merge = 0
        make_move_vec = 0
        # move calls
        move_call_count = 0
        packages = {}
        modules = {}
        functions = {}
        if txn.txn_type == TransactionType.PROGRAMMABLE_TRANSACTION:
            prog_txn = txn.programmable_transaction
            gas_info = prog_txn.gas_info
            move_calls = prog_txn.move_calls

            # gas
            computation_cost = gas_info.computation_cost
            computation_cost_rounded = gas_info.computation_cost_rounded
            gas_used = gas_info.gas_used
            storage_cost = gas_info.storage_cost
            storage_rebate = gas_info.storage_rebate
            non_refundable_storage_fee = gas_info.non_refundable_storage_fee
            instruction_count = gas_info.instruction_count
            stack_height = gas_info.stack_height
            stack_size = gas_info.stack_size
            # commands
            publish = prog_txn.publish[0]
            upgrade = prog_txn.upgrade
            transfer = prog_txn.transfer
            split = prog_txn.split
            merge = prog_txn.merge
            make_move_vec = prog_txn.make_move_vec
            # move calls
            move_call_count = 0
            for count in move_calls.functions.values():
                move_call_count += count
            packages = move_calls.packages
            modules = move_calls.modules
            functions = move_calls.functions

        normalized_txn.append(computation_cost)
        normalized_txn.append(computation_cost_rounded)
        normalized_txn.append(gas_used)
        normalized_txn.append(storage_cost)
        normalized_txn.append(storage_rebate)
        normalized_txn.append(non_refundable_storage_fee)
        normalized_txn.append(instruction_count)
        normalized_txn.append(stack_height)
        normalized_txn.append(stack_size)
        # commands
        normalized_txn.append(publish)
        normalized_txn.append(upgrade)
        normalized_txn.append(transfer)
        normalized_txn.append(split)
        normalized_txn.append(merge)
        normalized_txn.append(make_move_vec)
        # move calls
        normalized_txn.append(move_call_count)
        normalized_txn.append(packages)
        normalized_txn.append(modules)
        normalized_txn.append(functions)

        for t in txn.times:
            l = [t]
            l.extend(normalized_txn)
            normalized_list.append(l)

    return normalized_list