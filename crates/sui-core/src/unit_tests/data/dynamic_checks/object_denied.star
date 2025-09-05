def check_tx(tx_input_objects):
    for obj in tx_input_objects:
        imm_or_owned = obj.get("ImmOrOwnedMoveObject")
        if imm_or_owned and imm_or_owned[0] == "$OBJECT_ID":
            return False
    return True

check_tx(tx_input_objects)
