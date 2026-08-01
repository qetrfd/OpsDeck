#!/usr/bin/env sh

set -eu

mkdir -p "${HOME}/.opsdeck"

if command -v git >/dev/null 2>&1; then
    if ! git config \
        --global \
        --get-all safe.directory \
        2>/dev/null \
        | grep -Fqx "/workspace"
    then
        git config \
            --global \
            --add safe.directory \
            /workspace
    fi
fi

if [ "$#" -eq 0 ]; then
    set -- status /workspace
fi

exec /usr/local/bin/opsdeck "$@"
