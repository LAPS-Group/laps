use rocket::response::NamedFile;
use rocket_contrib::serve::StaticFiles;

//Export the admin module as pub if in test mode so any other tests which require a login can do so.
#[cfg(test)]
pub mod admin;
#[cfg(not(test))]
mod admin;

mod algorithms;
pub mod job;
mod map;
mod mime_consts;
pub mod multipart;

//Index stuff
#[get("/")]
fn index() -> Option<NamedFile> {
    NamedFile::open("dist/index.html").ok()
}

#[get("/index.js")]
fn index_js() -> Option<NamedFile> {
    NamedFile::open("dist/index.js").ok()
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
                admin::get_module_logs,
                admin::index,
                admin::index_js,
                admin::login,
                admin::login_index,
                admin::login_index_js,
                admin::new_map,
                admin::register_admin,
                admin::register_super_admin,
                admin::restart_module,
                admin::stop_module,
                admin::upload_module,
                algorithms::list,
                index,
                index_js,
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
