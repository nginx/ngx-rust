#!/bin/sh

set -ex

if [ $# -lt 2 ]; then
    grep "uses:[[:space:]]*$1" .github/workflows/*.yaml
    exit
fi

sed -e "s|\\(uses:[[:space:]]*$1@\\).*|\\1$2|" -i .github/workflows/*.yaml
