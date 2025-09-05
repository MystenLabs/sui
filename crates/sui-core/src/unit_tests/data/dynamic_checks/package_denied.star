def get_pt(tx_data):
    if (
        tx_data.get("V1") and
        tx_data["V1"].get("kind") and
        tx_data["V1"]["kind"].get("ProgrammableTransaction")
    ):
        return tx_data["V1"]["kind"]["ProgrammableTransaction"]
    else:
        return None

def has_denied_package(package_ids):
    for package_id in package_ids:
        if package_id == "$OBJECT_ID":
            return True
    return False

def check_tx(tx_data):
    pt = get_pt(tx_data)
    if not pt or not pt.get("commands"):
        return True
    for c in pt["commands"]:
        if c.get("Publish") and has_denied_package(c["Publish"][1]):
            return False
        if c.get("Upgrade") and has_denied_package(c["Upgrade"][1]):
            return False
        if c.get("MoveCall") and has_denied_package([c["MoveCall"]["package"]]):
            return False
    return True

check_tx(tx_data)
