#!/bin/bash

files=$(find frontend -iname '*.css' -or -iname '*.html' -or -iname '*.js' -or -iname '*.vue')

npx prettier -c $files
