import os
import sys

files = [
        "bytecode/address.move",
        "bytecode/bag.move",
        "bytecode/balance.move",
        "bytecode/bcs.move",
        "bytecode/clock.move",
        "bytecode/coin.move",
        "bytecode/devnet_nft.move",
        "bytecode/digest.move",
        "bytecode/dynamic_field.move",
        "bytecode/dynamic_object_field.move",
        "bytecode/epoch_time_lock.move",
        "bytecode/erc721_metadata.move",
        "bytecode/event.move",
        "bytecode/hex.move",
        "bytecode/immutable_external_resource.move",
        "bytecode/linked_table.move",
        "bytecode/locked_coin.move",
        "bytecode/math.move",
        "bytecode/object.move",
        "bytecode/object_bag.move",
        "bytecode/object_table.move",
        "bytecode/pay.move",
        "bytecode/priority_queue.move",
        "bytecode/publisher.move",
        "bytecode/safe.move",
        "bytecode/sui.move",
        "bytecode/table.move",
        "bytecode/table_vec.move",
        "bytecode/transfer.move",
        "bytecode/tx_context.move",
        "bytecode/typed_id.move",
        "bytecode/types.move",
        "bytecode/url.move",
        "bytecode/vec_map.move",
        "bytecode/vec_set.move",

        "bytecode/crypto/bls12381.move",
        "bytecode/crypto/bulletproofs.move",
        "bytecode/crypto/ecdsa_k1.move",
        "bytecode/crypto/ecdsa_r1.move",
        "bytecode/crypto/ecvrf.move",
        "bytecode/crypto/ed25519.move",
        "bytecode/crypto/elliptic_curve.move",
        "bytecode/crypto/groth16.move",
        "bytecode/crypto/hmac.move",
        "bytecode/crypto/randomness.move",
        "bytecode/crypto/hash.move",

        "bytecode/governance/genesis.move",
        "bytecode/governance/stake.move",
        "bytecode/governance/stake_subsidy.move",
        "bytecode/governance/staking_pool.move",
        "bytecode/governance/sui_system.move",
        "bytecode/governance/validator.move",
        "bytecode/governance/validator_set.move",
        "bytecode/governance/voting_power.move",
]

sys.stdout.write("{\n")
sys.stdout.write('    "source_files": [\n')
    
for i,f in enumerate(files):
#for f in ["sources/safe.move"]:
    module = f.split("/")[-1][:-5]
    # print(module)

    sys.stdout.write("        {\n")
    sys.stdout.write('            "name": "{}.bytecode",\n'.format(f))
    sys.stdout.write('            "coverage": [')
    sys.stdout.flush()
    #print("COV=1 ~/sui/target/debug/sui move coverage bytecode --module {}".format(module))
    os.system("COV=1 ~/sui/target/debug/sui move coverage bytecode --module {}".format(module))
    sys.stdout.write('            ]\n')
    if i != len(files)-1:
        sys.stdout.write("        },\n")
    else:
        sys.stdout.write("        }\n")
    sys.stdout.flush()

sys.stdout.write('    ]\n')
sys.stdout.write("}\n")    
sys.stdout.flush()
