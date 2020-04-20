#!/bin/sh

THIS_DIR=$PWD
cd $THIS_DIR/test_data/test_module
tar cvf test_module.tar main.py requirements.txt
mv test_module.tar ..

cd $THIS_DIR/test_data/instant_fail_test_module
tar cvf instant_fail_test_module.tar main.py requirements.txt
mv instant_fail_test_module.tar ..
