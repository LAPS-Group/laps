FROM amd64/python:slim-buster
RUN apt-get update
RUN apt-get upgrade -y
RUN python3 -m pip install redis

WORKDIR /module
COPY contents.tar contents.tar
COPY laps.py laps.py
RUN tar xvf contents.tar
RUN python3 -m pip install -r requirements.txt
