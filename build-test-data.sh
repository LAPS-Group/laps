#!/bin/sh

cd test_data/test_module
tar cvf test_module.tar main.py requirements.txt
mv test_module.tar ..
