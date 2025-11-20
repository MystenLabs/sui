# This is just here as an example of a shell test that creates a git repo to work against

# use default git settings
export GIT_CONFIG_GLOBAL=""

# Set up git repo for `a`
git init -q -b main a
git -C a add .
git -C a -c user.email=test@test.com -c user.name=test commit -q -m "initial revision"

HASH=$(git -C a log --pretty=format:%H)

sui move cache-package testnet 4c78adac "{ git = \"$(pwd)/a\", rev = \"${HASH}\", subdir = \".\" }"
