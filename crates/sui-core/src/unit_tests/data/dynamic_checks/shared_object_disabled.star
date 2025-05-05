def check_tx(tx_input_objects):
    for obj in tx_input_objects:
        if obj.get("SharedMoveObject"):
            return False
    return True

check_tx(tx_input_objects)
