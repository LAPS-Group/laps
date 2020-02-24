use rocket::response::NamedFile;

mod algorithms;
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

    rocket::ignite()
        .mount(
            "/",
            routes![
                dist,
                index,
                algorithms::list,
                job::result,
                job::submit,
                map::get_map,
                map::get_maps,
            ],
        )
        .manage(pool)
        .serve()
        .await
        .unwrap();
}
