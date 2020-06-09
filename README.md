# LAPS: Low-Altitude pathfinding service
This repository contains frontend and backend source code for the LAPS bachelor
project. We were a group of 4 students for the University of Southeastern Norway
(USN).

The service allows it's users to create pathfinding modules. These modules
consist of a pathfinding algorithm in some form. While intended for use with
machine learning algorithms, it can be used for any kind of pathfinding.

# Building and running
To pack the frontend, first install the npm packages with `npm i`. Then,

``` sh
npx run build_prod
```

This will bundle all the frontend code together. To build and run the service,
one needs an installation of the GDAL library. See the
[gdal-sys](https://github.com/georust/gdal/tree/master/gdal-sys) documentation
for more information.

Requires a nightly version of Rust. `cargo +nightly run` is enough to start the
service.
