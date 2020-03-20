use rocket::response::NamedFile;

mod admin;
mod algorithms;
mod job;
mod map;

//End points for getting the frontend code
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
    //Create the specialized pool for getting connection results
    let result_pool = job::create_result_redis_pool().await;
    //Launch module handlers
    tokio::spawn(crate::module_handling::run(pool.clone()));

    info!("Starting Rocket...");
    rocket::ignite()
        .mount(
            "/",
            routes![
                dist,
                index,
                admin::index,
                admin::login,
                admin::new_map,
                algorithms::list,
                job::result,
                job::submit,
                map::get_map,
                map::get_maps,
            ],
        )
        .manage(pool)
        .manage(result_pool)
        .serve()
        .await
        .unwrap();
}
