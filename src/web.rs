use warp::Filter;

pub async fn run() {
    let index = warp::get()
        .and(warp::path::end())
        .and(warp::fs::file("./dist/index.html"))
        .or(warp::path("dist.js").and(warp::fs::file("./dist/main.js")));

    warp::serve(index).run(([127, 0, 0, 1], 8000)).await
}
