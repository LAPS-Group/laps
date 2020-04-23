use rocket::response::NamedFile;
use rocket_contrib::serve::StaticFiles;

mod admin;
mod algorithms;
pub mod job;
mod map;
mod mime_consts;
mod multipart;

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
    //Connect to Docker
    let docker = crate::connect_to_docker().await;
    //Launch module handlers
    tokio::spawn(crate::module_handling::run(pool.clone()));

    info!("Starting Rocket...");
    rocket::ignite()
        .mount(
            "/",
            routes![
                admin::delete_map,
                admin::get_all_modules,
                admin::get_me,
                admin::index,
                admin::login,
                admin::new_map,
                admin::register_admin,
                admin::register_super_admin,
                admin::restart_module,
                admin::show_errors,
                admin::stop_module,
                admin::upload_module,
                algorithms::list,
                dist,
                index,
                job::result,
                job::submit,
                map::get_map,
                map::get_maps,
            ],
        )
        .mount("/images", StaticFiles::from("dist/images"))
        .manage(pool)
        .manage(result_pool)
        .manage(docker)
        .serve()
        .await
        .unwrap();
}
