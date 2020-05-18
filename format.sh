#!/bin/bash

# format.sh: Fix or check the formatting of frontend code
# Author: Håkon Jordet
# Copyright (c) 2020 LAPS Group
# Distributed under the zlib licence, see LICENCE.

files=$(find frontend -iname '*.css' -or -iname '*.html' -or -iname '*.js' -or -iname '*.vue')

echo "Found $files"

case "$1" in
    "check")
        npx prettier -c $files || exit 1
        npx prettier -c webpack.config.js || exit 1
        ;;
    "fix")
        npx prettier --write $files
        npx prettier --write webpack.config.js
        ;;
esac
