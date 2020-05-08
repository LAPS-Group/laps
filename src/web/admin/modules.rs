use super::mime_consts;
use super::AdminSession;
use crate::{
    module_handling::ModuleInfo,
    types::{BackendError, UserError},
    util,
    web::multipart::{FormError, MultipartForm},
};
use bollard::{
    container::{
        APIContainers, Config, CreateContainerOptions, HostConfig, InspectContainerOptions,
        ListContainersOptions, RemoveContainerOptions, RestartContainerOptions,
        StartContainerOptions, StopContainerOptions,
    },
    errors::ErrorKind,
    image::{
        APIImages, BuildImageOptions, BuildImageResults, ListImagesOptions, RemoveImageOptions,
        RemoveImageResults,
    },
    Docker,
};
use darkredis::ConnectionPool;
use futures::stream::{StreamExt, TryStreamExt};
use rocket::{
    http::{ContentType, Status},
    request::State,
    Response,
};
use rocket_contrib::json::Json;
use serde::{Deserialize, Serialize};
use std::io::Cursor;

#[get("/module/<name>/<version>/logs")]
pub async fn get_module_logs<'a>(
    pool: State<'a, ConnectionPool>,
    docker: State<'a, Docker>,
    name: String,
    version: String,
    _session: AdminSession,
) -> Result<Response<'a>, BackendError> {
    //Find out if the module exists
    let module = ModuleInfo { name, version };
    if module_exists(&docker, &module).await? {
        let mut conn = pool.get().await;
        let log_key = util::get_module_log_key(&module);
        //Get all the elements of the log and concatenate them.
        let out =
            conn.lrange(log_key, 0, -1)
                .await?
                .into_iter()
                .fold(Vec::new(), |mut out, mut x| {
                    out.append(&mut x);
                    out.push(b'\n');
                    out
                });

        let cursor = Cursor::new(out);
        Ok(Response::build()
            .status(Status::Ok)
            .header(ContentType::Plain)
            .sized_body(cursor)
            .await
            .finalize())
    } else {
        Ok(Response::build().status(Status::NotFound).finalize())
    }
}

//Enum describing the state of a module or container.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "state")]
pub enum ModuleState {
    Running,
    Stopped,
    Failed { exit_code: i32 },
    //A module that is partially stopped or failed.
    Other { message: String },
}

//Return value for the module structs, with an additional field to determine if a module is currently running.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct PathModule {
    #[serde(flatten)]
    pub state: ModuleState,
    #[serde(flatten)]
    pub module: ModuleInfo,
}

fn extract_module_info_from_tag(tag: &str) -> Option<ModuleInfo> {
    //A valid tag will always have the format "a:b"
    tag.find(':')
        .map(|s| {
            let module = ModuleInfo {
                name: tag[..s].to_string(),
                version: tag[s + 1..].to_string(),
            };
            //Ignore untagged modules
            if module.name != "<none>" {
                Some(module)
            } else {
                None
            }
        })
        .flatten()
}

//Get a list of the running modules
async fn running_modules(docker: &Docker) -> Result<Vec<ModuleInfo>, BackendError> {
    Ok(docker
        .list_containers(None::<ListContainersOptions<String>>)
        .await?
        .into_iter()
        .map(|s| extract_module_info_from_tag(&s.image).unwrap())
        .collect())
}

//Get all modules along with their container options.
async fn list_all_modules(
    docker: &Docker,
) -> Result<Vec<(ModuleInfo, APIContainers)>, BackendError> {
    let options = ListContainersOptions::<String> {
        all: true,
        ..Default::default()
    };
    Ok(docker
        .list_containers(Some(options))
        .await?
        .into_iter()
        .filter_map(|m| extract_module_info_from_tag(&m.image).map(|i| (i, m)))
        .collect())
}

//Check if a module exists.
pub async fn module_exists(docker: &Docker, module: &ModuleInfo) -> Result<bool, BackendError> {
    //Get a list of all modules
    let images: Vec<APIImages> = docker
        .list_images(None::<ListImagesOptions<String>>)
        .await
        .map_err(BackendError::Docker)?;
    //Figure out if module with name `name` and version `version` is in that list.
    Ok(images.into_iter().any(|i| {
        if let Some(t) = i.repo_tags {
            t.into_iter()
                .map(|s| extract_module_info_from_tag(&s))
                .any(|s| s.as_ref() == Some(module))
        } else {
            false
        }
    }))
}

//Check if a module is running
pub async fn module_is_running(docker: &Docker, module: &ModuleInfo) -> Result<bool, BackendError> {
    let running_modules = running_modules(&docker).await?;
    Ok(running_modules.iter().any(|m| m == module))
}

//Get a pathfinding module's state from `container`.
fn get_container_state(container: &APIContainers) -> ModuleState {
    match container.state.as_str() {
        "running" => ModuleState::Running,
        "exited" => {
            //If exited, check the exit code. There doesn't seem to be a good way to do this,
            //so assume that the format won't change.
            //The format looks like "Exited (code) [...]" where `code` is the exit code.

            //Find the first parenthesis.
            if let Some(p) = container.status.find('(') {
                //Assume that the format is correct if we got here
                let second_par = container.status[p..].find(')').unwrap();
                //Extract the code itself from the string.
                let exit_code: i32 = container.status[p + 1..p + second_par].parse().unwrap();
                //Following UNIX conventions, a 0 exit value indicates success
                if exit_code == 0 {
                    ModuleState::Stopped
                } else {
                    ModuleState::Failed { exit_code }
                }
            } else {
                //We should always be able to find the parenthesis, but if it fails,
                //just ignore the error and say that it's stopped, because that is still correct.
                error!(
                    "Couldn't find '(' in container status: {}",
                    container.status
                );
                ModuleState::Stopped
            }
        }
        _ => unreachable!(),
    }
}

#[get("/module/all")]
pub async fn get_all_modules(
    docker: State<'_, Docker>,
    _session: AdminSession,
) -> Result<Json<Vec<PathModule>>, BackendError> {
    //Mostly just list available docker images to create
    let images: Vec<APIImages> = docker
        .list_images(None::<ListImagesOptions<String>>)
        .await?;

    let all_modules = list_all_modules(&docker).await?;

    let mut out = Vec::new();
    for image in images {
        //For each tag, grab the module information so that we display all modules, even those with identical images.
        if let Some(tags) = image.repo_tags {
            for tag in tags {
                //If there is no module info for this image, this can fail. `ApiImage::repo_tags`
                //has a confusing type signature for sure...
                let module = match extract_module_info_from_tag(&tag) {
                    Some(m) => m,
                    None => continue,
                };

                //Skip this module if it is in the ignore list.
                if (*crate::CONFIG).module.ignore.contains(&module.name) {
                    continue;
                }

                //Get the state of all containers with this tag, i.e all containers created from the same module image.
                //And fold it into  a containerstates struct.
                let states: Vec<ModuleState> = all_modules
                    .iter()
                    .filter_map(|(m, container)| {
                        if m == &module {
                            Some(get_container_state(&container))
                        } else {
                            None
                        }
                    })
                    .collect();
                //If we found no containers, the module was never started.
                let state = if states.is_empty() {
                    ModuleState::Stopped
                } else {
                    //If all containers have the same state we can just forward that.
                    let last = states.first().unwrap(); // already did the bounds check.
                    if states.iter().all(|s| s == last) {
                        last.clone()
                    } else {
                        //If not we have to build the response string.
                        //Struct containing the state of all the containers.
                        #[derive(Default)]
                        struct ContainerStates {
                            running: i32,
                            stopped: i32,
                            failed: i32,
                            exit_codes: Vec<i32>,
                        };
                        let mut states = states.into_iter().fold(
                            ContainerStates::default(),
                            |mut acc, state| {
                                match state {
                                    ModuleState::Running => acc.running += 1,
                                    ModuleState::Stopped => acc.stopped += 1,
                                    ModuleState::Failed { exit_code } => {
                                        acc.failed += 1;
                                        acc.exit_codes.push(exit_code);
                                    }
                                    //The only way for this to happen is if the get_container_state function is broken
                                    _ => unreachable!(),
                                }
                                acc
                            },
                        );
                        //Avoid duplicates in the exit codes
                        states.exit_codes.sort_unstable();
                        states.exit_codes.dedup();

                        //Convert the states into a nice string
                        let workers = states.running + states.stopped + states.failed;
                        let mut message = format!("{}/{} running", states.running, workers);
                        if states.stopped > 0 {
                            message += &format!(", {} stopped", states.stopped);
                        }
                        if states.failed > 0 {
                            message += &format!(
                                ", {} failures with exit codes {:?}",
                                states.failed, states.exit_codes
                            );
                        }
                        ModuleState::Other { message }
                    }
                };

                out.push(PathModule { module, state });
            }
        }
    }
    Ok(Json(out))
}

#[post("/module", data = "<form>")]
pub async fn upload_module(
    mut form: MultipartForm,
    pool: State<'_, ConnectionPool>,
    docker: State<'_, Docker>,
    session: AdminSession,
) -> Result<Status, UserError> {
    //Include the module runner dependencies into the executable to make managing them easier.
    const MODULE_DOCKERFILE: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/laps_module_runner/Dockerfile"
    ));
    const MODULE_LAPS_PY: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/laps_module_runner/laps.py"
    ));

    //Get the required fields out of the form.
    let name = form.get_text("name")?.trim().to_string();
    let version = form.get_text("version")?.trim().to_string();

    //This field is optional and determines how many instances of the module we can run at once.
    //If the field doesn't exist, assume 1.
    let concurrent_workers = match form.get_text("workers").map(|s| s.parse::<u8>()) {
        Ok(Ok(n)) => n,
        Ok(Err(e)) => {
            warn!("Failed to parse worker count: {}", e);
            return Err(UserError::BadForm(FormError::Other(
                "Invalid worker count".into(),
            )));
        }
        Err(FormError::MissingText(_)) => 1,

        Err(e) => {
            return Err(UserError::BadForm(e));
        }
    };

    //Accept only .tar
    let module = form.get_file(&mime_consts::X_TAR, "module")?;

    //Validation
    //Check the name and version for invalid characters
    if name.chars().any(|c| c == ':') || version.chars().any(|c| c == ':') {
        return Err(UserError::ModuleImport(
            "Neither name nor version cannot contain ':'".into(),
        ));
    }

    //Check that there's no image with the same name and version currently
    //Docker only accepts lowercase names so do that automatically.
    let info = ModuleInfo {
        name: name.to_lowercase(),
        version: version.to_lowercase(),
    };
    if module_exists(&docker, &info).await? {
        return Err(UserError::ModuleImport("Module already exists".into()));
    }

    //Time to create the image, pack it all into a tar:
    let mut tarball = Vec::new();
    {
        //use an inner scope to drop `builder` when we're done.
        let mut builder = tar::Builder::new(&mut tarball);
        //Insert LAPS files. Insertion of data cannot fail because we are writing directly to memory.
        let mut header = tar::Header::new_gnu();
        header.set_size(MODULE_DOCKERFILE.len() as u64);
        builder
            .append_data(&mut header, "Dockerfile", MODULE_DOCKERFILE)
            .unwrap();
        header.set_size(MODULE_LAPS_PY.len() as u64);
        builder
            .append_data(&mut header, "laps.py", MODULE_LAPS_PY)
            .unwrap();

        //Finally append the user data to the archive
        header.set_size(module.len() as u64);
        builder
            .append_data(&mut header, "contents.tar", module.as_slice())
            .unwrap();

        builder.finish().expect("writing image tarball");
    }

    //Build the image
    let options = BuildImageOptions {
        t: format!("{}:{}", info.name, info.version),
        rm: true,
        forcerm: true,
        ..Default::default()
    };
    let mut stream = docker.build_image(options, None, Some(tarball.into()));
    while let Some(update) = stream.next().await {
        let update = update.map_err(|e| {
            error!("Error getting image build output: {:?}", e);
            UserError::ModuleImport(e.to_string())
        })?;

        debug!("Importing {}: {:?}", info, update);
        if let BuildImageResults::BuildImageError {
            error,
            error_detail,
        } = update
        {
            return Err(UserError::ModuleImport(format!(
                "Module import error: {}\nDetails: {:?}",
                error, error_detail
            )));
        }
    }

    //Now that everything has succeeded, store the number of jobs we can use in the database.
    //This shouldn't fail, but if it does, return an error.
    let mut redis = pool.get().await;
    let key = util::get_module_workers_key(&info);
    match redis.set(&key, concurrent_workers.to_string()).await {
        Ok(()) => (),
        Err(e) => {
            error!("Failed to set worker count for {}: {}", info, e);
            return Err(UserError::Internal(BackendError::Redis(e)));
        }
    };

    info!("{} imported module {}", session.username, info);
    Ok(Status::Created)
}

#[post("/module/<name>/<version>/restart")]
pub async fn restart_module(
    session: AdminSession,
    name: String,
    version: String,
    docker: State<'_, Docker>,
    pool: State<'_, ConnectionPool>,
) -> Result<Status, BackendError> {
    //First, verify that the requested module actually exists:
    let module = ModuleInfo { name, version };
    if !module_exists(&docker, &module).await? {
        return Ok(Status::NotFound);
    }

    //Get the number of concurrent workers allowed for this module without hogging the Redis connection.
    let concurrent_workers = {
        let mut conn = pool.get().await;
        conn.get(&util::get_module_workers_key(&module))
            .await?
            .map(|s| String::from_utf8_lossy(&s).parse::<u8>().unwrap())
            .expect("getting worker number field")
    };

    //If the module is already running, use the restart_container method
    let container_name = module.to_string().replace(":", "-");
    if module_is_running(&docker, &module).await? {
        //It might take a while to restart a module as it will have to have time to exit.
        //To get around this, perform each restart concurrently.
        futures::stream::iter(0..concurrent_workers)
            .map(Ok)
            .try_for_each_concurrent(None, |n| {
                let docker = docker.clone();
                let session = session.clone();
                let module = module.clone();
                let container_name = format!("{}-{}", container_name, n);
                async move {
                    trace!("Restarting {} worker {}", session.username, &module);
                    //Give the module 30s to shut down
                    let options = RestartContainerOptions { t: 30 };
                    match docker
                        .restart_container(&container_name, Some(options))
                        .await
                    {
                        Ok(_) => {
                            info!(
                                "{} restarted module {} worker {}",
                                session.username, &module, n
                            );
                            Ok(())
                        }
                        Err(e) => {
                            error!("Failed to restart module {} worker {}: {}", &module, n, e);
                            Err(e)
                        }
                    }
                }
            })
            .await?;
        Ok(Status::NoContent)
    } else {
        //If containers have already been created for the module, do not try to recreate them.
        let options = ListContainersOptions::<String> {
            all: true,
            ..Default::default()
        };
        let containers_exist = docker
            .list_containers(Some(options))
            .await?
            .into_iter()
            .any(|c| {
                //When we receive the container names from Docker, they all start with a `/` for some reason.
                c.names
                    .into_iter()
                    .any(|s| s[1..].starts_with(&container_name))
            });
        if !containers_exist {
            //No containers have been created yet, build them up
            debug!("Creating containers for module {}", container_name);
            let redis = &crate::CONFIG.redis.address;
            //For Redis to succeed in connecting the format of the address field must be <host>:<port>
            let split = redis.find(':').unwrap();
            let redis_host = &redis[..split];
            let redis_port = &redis[split + 1..];

            for worker_number in (0..concurrent_workers).map(|w| w.to_string()) {
                //Run it with a default set of commands
                let mut command = vec![
                    "python3",
                    "main.py",
                    &module.name,
                    &module.version,
                    "--redis_host",
                    redis_host,
                    "--port",
                    redis_port,
                    "--worker_number",
                    &worker_number,
                ];
                //Use test keys in laps.py if running in test mode
                if cfg!(test) {
                    command.push("--test");
                }

                //Setup the settings
                let module_name = module.to_string();
                let host_config = HostConfig {
                    network_mode: Some("host"),
                    ..Default::default()
                };
                let config = Config {
                    image: Some(module_name.as_str()),
                    cmd: Some(command),
                    host_config: Some(host_config),
                    stop_signal: Some("SIGINT"),
                    ..Default::default()
                };
                let this_worker_name = format!("{}-{}", container_name, worker_number);
                let options = CreateContainerOptions {
                    name: &this_worker_name,
                };
                //Print any warnings
                let result = docker.create_container(Some(options), config).await?;
                debug!(
                    "Successfully created container {}:{}",
                    this_worker_name, result.id
                );
                let id = &result.id;
                if let Some(w) = result.warnings {
                    w.into_iter().for_each(|w| warn!("Container {}: {}", id, w));
                }
            }
        }

        //Finally start all the containers:
        for worker_number in 0..concurrent_workers {
            let this_worker_name = format!("{}-{}", container_name, worker_number);
            docker
                .start_container(&this_worker_name, None::<StartContainerOptions<String>>)
                .await?;
            debug!("Successfully started container {}", this_worker_name);
        }
        info!(
            "{} successfully started module {}",
            session.username, module
        );
        Ok(Status::Created)
    }
}

#[post("/module/<name>/<version>/stop")]
pub async fn stop_module(
    session: AdminSession,
    name: String,
    version: String,
    docker: State<'_, Docker>,
    pool: State<'_, ConnectionPool>,
) -> Result<Status, BackendError> {
    //If the module doesn't exist, 404
    let module = ModuleInfo { name, version };
    if !module_exists(&docker, &module).await? {
        warn!("Couln't find module {}", module);
        Ok(Status::NotFound)
    } else {
        //If the module isn't running, don't bother stopping it
        if !module_is_running(&docker, &module).await? {
            Ok(Status::BadRequest)
        } else {
            let options = StopContainerOptions { t: 60 };
            let container = module.to_string().replace(":", "-");
            let mut conn = pool.get().await;
            let num_workers = String::from_utf8_lossy(
                &conn
                    .get(util::get_module_workers_key(&module))
                    .await?
                    .expect("getting number of workers"),
            )
            .parse::<u8>()
            .unwrap();
            for worker in 0..num_workers {
                let worker_container = format!("{}-{}", container, worker);
                match docker
                    .stop_container(&worker_container, Some(options))
                    .await
                {
                    Ok(_) => {
                        debug!("Stopped container {}", worker_container);
                    }
                    Err(e) => {
                        error!(
                            "Failed attempt to stop {} by {}: {:?}",
                            container, session.username, e
                        );
                        return Err(BackendError::Docker(e));
                    }
                }
            }
            info!("module {} stopped by {}", container, session.username);
            Ok(Status::NoContent)
        }
    }
}

#[delete("/module/<name>/<version>")]
pub async fn delete_module(
    session: AdminSession,
    name: String,
    version: String,
    docker: State<'_, Docker>,
    pool: State<'_, ConnectionPool>,
) -> Result<Response<'static>, BackendError> {
    //Refuse to delete a module if it does not exist or is currently running
    let module = ModuleInfo { name, version };
    if !module_exists(&docker, &module).await? {
        return Ok(Response::build().status(Status::NotFound).finalize());
    }
    if module_is_running(&docker, &module).await? {
        return Ok(Response::build()
            .status(Status::BadRequest)
            .sized_body(Cursor::new("Cannot delete a running module!"))
            .await
            .finalize());
    }

    //Now we can delete the module. First off, the containers have to be deleted.

    //Assume that if the first container exists that the rest do.
    let result = docker
        .inspect_container(
            &format!("{}-{}-0", module.name, module.version),
            None::<InspectContainerOptions>,
        )
        .await;
    let containers_exist = match result {
        Ok(_) => true,
        Err(e) => match e.kind() {
            ErrorKind::DockerResponseNotFoundError { .. } => false,
            _ => return Err(BackendError::Docker(e)),
        },
    };

    //Delete the containers if they exist.
    if containers_exist {
        let workers = {
            let mut conn = pool.get().await;
            conn.get(util::get_module_workers_key(&module))
                .await
                .expect("getting desired worker count")
                .map(|s| String::from_utf8_lossy(&s).parse::<u8>().unwrap())
                .unwrap()
        };
        for w in 0..workers {
            let this_container = format!("{}-{}-{}", module.name, module.version, w);
            docker
                .remove_container(&this_container, None::<RemoveContainerOptions>)
                .await?;
            debug!("Removed container {}", this_container);
        }
    }

    //Remove all traces of the module from the database.
    {
        let mut conn = pool.get().await;
        let keys = vec![
            util::get_module_log_key(&module),
            util::get_module_workers_key(&module),
            util::get_registered_module_workers_key(&module),
            util::get_module_work_key(&module),
        ];
        let deleted = conn.del_slice(&keys).await?;
        debug!("Removed {} database entries related to {}", deleted, module);
    }

    //Get the number of workers for this module
    let options = RemoveImageOptions {
        force: true,
        noprune: false,
    };
    let image_deletions = docker
        .remove_image(&module.to_string(), Some(options), None)
        .await?;
    //Output the deletions if debug log is active
    if log_enabled!(log::Level::Debug) {
        for deletion in image_deletions {
            match deletion {
                RemoveImageResults::RemoveImageUntagged { untagged } => {
                    debug!("Untagged {}", untagged);
                }
                RemoveImageResults::RemoveImageDeleted { deleted } => {
                    debug!("Deleted {}", deleted);
                }
            }
        }
    }

    info!("Module {} deleted by {}", module, session.username);

    Ok(Response::build().status(Status::NoContent).finalize())
}
