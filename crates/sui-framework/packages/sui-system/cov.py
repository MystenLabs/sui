import os
import sys

BIN = "/Users/rijnard/sui/target/debug/sui"

files = [
#        "sui-framework/sources/address.move",
#        "sui-framework/sources/bag.move",
#        "sui-framework/sources/balance.move",
#        "sui-framework/sources/bcs.move",
#        "sui-framework/sources/borrow.move",
#        "sui-framework/sources/clock.move",
#        "sui-framework/sources/coin.move",
#        "sui-framework/sources/display.move",
#        "sui-framework/sources/dynamic_field.move",
#        "sui-framework/sources/dynamic_object_field.move",
#        "sui-framework/sources/event.move",
#        "sui-framework/sources/hex.move",
#        "sui-framework/sources/linked_table.move",
#        "sui-framework/sources/math.move",
#        "sui-framework/sources/object.move",
#        "sui-framework/sources/object_bag.move",
#        "sui-framework/sources/object_table.move",
#        "sui-framework/sources/package.move",
#        "sui-framework/sources/pay.move",
#        "sui-framework/sources/priority_queue.move",
#        "sui-framework/sources/prover.move",
#        "sui-framework/sources/sui.move",
#        "sui-framework/sources/table.move",
#        "sui-framework/sources/table_vec.move",
        
#        "sui-framework/sources/transfer.move",
#        "sui-framework/sources/tx_context.move",
#        "sui-framework/sources/types.move",
#        "sui-framework/sources/url.move",
#        "sui-framework/sources/vec_map.move",
#        "sui-framework/sources/vec_set.move",
#        "sui-framework/sources/versioned.move",

#        "sui-framework/sources/crypto/bls12381.move",
#        "sui-framework/sources/crypto/ecdsa_k1.move",
#        "sui-framework/sources/crypto/ecdsa_r1.move",
#        "sui-framework/sources/crypto/ecvrf.move",
#        "sui-framework/sources/crypto/ed25519.move",
#        "sui-framework/sources/crypto/groth16.move",
#        "sui-framework/sources/crypto/hash.move",
#        "sui-framework/sources/crypto/hmac.move",

#        "sui-framework/sources/kiosk/kiosk.move",
#        "sui-framework/sources/kiosk/transfer_policy.move",

        "sui-system/sources/genesis.move",
        "sui-system/sources/stake_subsidy.move",
        "sui-system/sources/staking_pool.move",
        "sui-system/sources/storage_fund.move",
        "sui-system/sources/sui_system.move",
        "sui-system/sources/sui_system_state_inner.move",
        "sui-system/sources/validator.move",
        "sui-system/sources/validator_cap.move",
        "sui-system/sources/validator_set.move",
        "sui-system/sources/validator_wrapper.move",
        "sui-system/sources/voting_power.move",
]

sys.stdout.write("{\n")
sys.stdout.write('    "source_files": [\n')
    
for i,f in enumerate(files):
    module = f.split("/")[-1][:-5]
    f = f[:-5] # strip .move
    # print(module)

    sys.stdout.write("        {\n")
    sys.stdout.write('            "name": "{}.mv.bytecode",\n'.format(f))
    sys.stdout.write('            "coverage": [')
    sys.stdout.flush()
    # print("COV=1 {} move coverage bytecode --module {}".format(BIN, module))
    os.system("COV=1 {} move coverage bytecode --module {}".format(BIN, module))
    sys.stdout.write('            ]\n')
    if i != len(files)-1:
        sys.stdout.write("        },\n")
    else:
        sys.stdout.write("        }\n")
    sys.stdout.flush()

sys.stdout.write('    ]\n')
sys.stdout.write("}\n")    
sys.stdout.flush()
