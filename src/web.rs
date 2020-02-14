use warp::Filter;

pub async fn run() {
    let index = warp::get()
        .and(warp::path::end())
        .and(warp::fs::file("./dist/index.html"))
        .or(warp::path("dist.js").and(warp::fs::file("./dist/main.js")));

    let routes = index;
    let config = &crate::CONFIG.web;
    let address = std::net::SocketAddr::new(config.address, config.port);
    warp::serve(routes).run(address).await
}
