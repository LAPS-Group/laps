use rocket::response::NamedFile;

mod job;
mod map;

#[get("/")]
fn index() -> Option<NamedFile> {
    NamedFile::open("dist/index.html").ok()
}

#[get("/main.js")]
fn dist() -> Option<NamedFile> {
    NamedFile::open("dist/main.js").ok()
}

//Launch the rocket instance
pub async fn run() {
    let pool = crate::create_redis_pool().await;
    //Launch module handlers
    tokio::spawn(crate::module_handling::run(pool.clone()));

    info!("Starting Rocket...");
    rocket::ignite()
        .mount(
            "/",
            routes![dist, index, map::get_maps, map::get_map, job::submit],
        )
        .manage(pool)
        .serve()
        .await
        .unwrap();
}
