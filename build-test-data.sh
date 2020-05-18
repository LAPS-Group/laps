#!/bin/sh

# build-test-data.sh: Package test modules for testing the backend.
# Author: Håkon Jordet
# Copyright (c) 2020 LAPS Group
# Distributed under the zlib licence, see LICENCE.

# This script kinda sucks but it's OK because nobody will see it :^)
# Might be replaceable with a for loop and some trickery but my shell-fu couldn't figure it out
# before giving up.

THIS_DIR=$PWD
cd $THIS_DIR/test_data/test_modules/simple
tar cvf simple.tar main.py requirements.txt
mv simple.tar ..

cd $THIS_DIR/test_data/test_modules/instant_fail
tar cvf instant_fail.tar main.py requirements.txt
mv instant_fail.tar ..

cd $THIS_DIR/test_data/test_modules/failing
tar cvf failing.tar main.py requirements.txt
mv failing.tar ..
