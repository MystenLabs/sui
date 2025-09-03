def get_pt(tx_data):
    if (
        tx_data.get("V1") and
        tx_data["V1"].get("kind") and
        tx_data["V1"]["kind"].get("ProgrammableTransaction")
    ):
        return tx_data["V1"]["kind"]["ProgrammableTransaction"]
    else:
        return None

def check_tx(tx_data):
    pt = get_pt(tx_data)
    if not pt or not pt.get("commands"):
        return True
    for c in pt["commands"]:
        if c.get("Publish"):
            return False
    return True

check_tx(tx_data)
