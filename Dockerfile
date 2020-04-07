FROM rustlang/rust:nightly-buster-slim
RUN ["apt-get", "update"]
RUN ["apt-get", "install", "build-essential", "-y"]
RUN ["apt-get", "install", "libclang-dev", "-y"]
RUN ["apt-get", "install", "libgdal-dev", "-y"]
RUN ["apt-get", "install", "clang", "-y"]

WORKDIR /workdir
ADD . /workdir
 
CMD ["cargo", "+nightly", "build", "--release"]
