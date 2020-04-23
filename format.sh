#!/bin/bash

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
