import os
import subprocess
import requests

def check_git_installation():
    try:
        subprocess.check_output(["git", "--version"])
        return True
    except FileNotFoundError:
        return False

def check_network_connection():
    try:
        requests.get("https://github.com", timeout=5)
        return True
    except requests.ConnectionError:
        return False

def git_clone_repository(repo_url):
    process = subprocess.Popen(
        ["git", "clone", repo_url],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True
    )
    while True:
        output = process.stdout.readline()
        if output == '' and process.poll() is not None:
            break
        if output:
            print(output.strip())
    rc = process.poll()
    return rc

if not check_git_installation():
    git_path = input("Git is not installed. Please specify the Git installation path or install Git: ")
    os.environ["PATH"] += os.pathsep + git_path
    if not check_git_installation():
        print("Git installation not found. Please install Git.")
        exit(1)

if not check_network_connection():
    print("Network connection failed. Please check your internet connection.")
    exit(1)

repo_url = "https://github.com/MystenLabs/sui.git"
print("Cloning repository, please wait...")
if git_clone_repository(repo_url) == 0:
    print("Repository cloned successfully.")
else:
    print("Failed to clone repository.")
