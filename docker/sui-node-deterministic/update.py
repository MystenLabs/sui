#!/usr/bin/env python3
from requests import Session
from fileinput import FileInput

target = "Dockerfile"
source = "https://codeberg.org/stagex/stagex/raw/branch/main/digests/"
stages = ["core","user","bootstrap","pallet"]

digests = {}
for stage in stages:
    response = Session().get(f"{source}{stage}.txt")
    for line in response.iter_lines():
        if not line:
            continue
        (digest, name) = line.decode("utf-8").split(" ")
        print(digest, name)
        digests[name] = digest

with FileInput(target, inplace=True, backup='.bak') as f:
    for line in f:
        if line.startswith("FROM stagex/"):
            # NOTE: split by '@' in case a tag is not provided
            # Matches:
            # stagex/tag:version@sha256:hash
            # stagex/tag@sha256:hash
            # stagex/tag:version
            # stagex/tag
            name = line.split("/")[1].split(":")[0].split('@')[0]
            if name not in digests:
                for stage in stages:
                    if f"{stage}-{name}" in digests:
                        name = f"{stage}-{name}"
            print(f"FROM stagex/{name}@sha256:{digests[name]} AS {name}")
        else:
            print(line,end='')
