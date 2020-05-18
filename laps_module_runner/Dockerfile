# laps_module_runner/Dockerfile: Dockerfile used to build pathfinding module images
# Author: Håkon Jordet
# Copyright (c) 2020 LAPS Group
# Distributed under the zlib licence, see LICENCE.

FROM amd64/python:slim-buster
RUN apt-get update
RUN apt-get upgrade -y
RUN python3 -m pip install redis

WORKDIR /module
COPY contents.tar contents.tar
COPY laps.py laps.py
RUN tar xvf contents.tar
RUN python3 -m pip install -r requirements.txt
