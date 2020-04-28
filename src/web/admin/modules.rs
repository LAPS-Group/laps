use super::mime_consts;
use super::AdminSession;
use crate::{
    module_handling::ModuleInfo,
    types::{BackendError, UserError},
    util,
    web::multipart::MultipartForm,
};
use bollard::{
    container::{
        APIContainers, Config, CreateContainerOptions, HostConfig, ListContainersOptions,
        RestartContainerOptions, StartContainerOptions, StopContainerOptions,
    },
    image::{APIImages, BuildImageOptions, BuildImageResults, ListImagesOptions},
    Docker,
};
use darkredis::ConnectionPool;
use rocket::{
    http::{ContentType, Status},
    request::State,
    Response,
};
use rocket_contrib::json::Json;
use serde::{Deserialize, Serialize};
use std::io::Cursor;
use tokio::stream::StreamExt;

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

        //If empty return 204 no content
        if out.is_empty() {
            Ok(Response::build().status(Status::NoContent).finalize())
        } else {
            let cursor = Cursor::new(out);
            Ok(Response::build()
                .status(Status::Ok)
                .header(ContentType::Plain)
                .sized_body(cursor)
                .await
                .finalize())
        }
    } else {
        Ok(Response::build().status(Status::NotFound).finalize())
    }
}

#[derive(Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "state")]
pub enum ModuleState {
    Running,
    Stopped,
    Failed { exit_code: i32 },
}

//Return value for the module structs, with an additional field to determine if a module is currently running.
#[derive(Serialize, Deserialize, PartialEq)]
pub struct PathModule {
    #[serde(flatten)]
    pub state: ModuleState,
    #[serde(flatten)]
    pub module: ModuleInfo,
}

fn extract_module_info_from_tag(tag: &str) -> Option<ModuleInfo> {
    //A valid tag will always have the format "a:b"
    tag.find(':').map(|s| ModuleInfo {
        name: tag[..s].to_string(),
        version: tag[s + 1..].to_string(),
    })
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
        //We are are guaranteed to have a version if repo_tags is Some.
        if let Some(m) = i.repo_tags.map(|s| {
            s.last()
                .map(|l| extract_module_info_from_tag(l).unwrap())
                .unwrap()
        }) {
            module == &m
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
                ModuleState::Failed { exit_code }
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
        //The repo_tags field will always have at least one element in it if it's `Some`.
        if let Some(tag) = image.repo_tags.map(|mut t| t.pop().unwrap()) {
            //A valid tag created by the backend will always have a version.
            let module = extract_module_info_from_tag(&tag).unwrap();

            //Look for a container associated with this image.
            let state = match all_modules.iter().find(|(m, _)| m == &module) {
                Some((_, container)) => {
                    //Found a container. Check it's state to return the proper status.
                    get_container_state(&container)
                }
                //If there's no container associated with the image, it simply hasn't been created yet,
                //and we can just say that it's stopped.
                None => ModuleState::Stopped,
            };

            out.push(PathModule { state, module });
        }
    }
    Ok(Json(out))
}

#[post("/module", data = "<form>")]
pub async fn upload_module(
    mut form: MultipartForm,
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
    let name = form
        .get_text("name")
        .ok_or_else(|| UserError::BadForm("Missing name".into()))?
        .trim()
        .to_string();
    let version = form
        .get_text("version")
        .ok_or_else(|| UserError::BadForm("Missing version".into()))?
        .trim()
        .to_string();

    //Accept only .tar
    let module = form
        .get_file(&mime_consts::X_TAR, "module")
        .ok_or_else(|| UserError::BadForm("Expected module with type application/x-tar".into()))?;

    //Validation
    //Check the name
    if name.chars().any(|c| c == ':') {
        return Err(UserError::BadForm("Name cannot have ':' in it".into()));
    }

    //Check that there's no image with the same name and version currently
    let info = ModuleInfo { name, version };
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

    info!("{} imported module {}", session.username, info);
    Ok(Status::Created)
}

#[post("/module/<name>/<version>/restart")]
pub async fn restart_module(
    session: AdminSession,
    name: String,
    version: String,
    docker: State<'_, Docker>,
) -> Result<Status, BackendError> {
    //First, verify that the requested module actually exists:
    let module = ModuleInfo { name, version };
    if !module_exists(&docker, &module).await? {
        return Ok(Status::NotFound);
    }

    //If the module is already running, use the restart_container method
    let container_name = module.to_string().replace(":", "-");
    if module_is_running(&docker, &module).await? {
        //Give the module 30s to shut down
        let options = RestartContainerOptions { t: 30 };
        docker
            .restart_container(&container_name, Some(options))
            .await?;
        info!("{} restarted module {}", session.username, &module);
        Ok(Status::NoContent)
    } else {
        //If a container has already been created for the module, do not create a container with the same name again.
        let options = ListContainersOptions::<String> {
            all: true,
            ..Default::default()
        };
        dbg!(&container_name);
        let container_exists = docker
            .list_containers(Some(options))
            .await?
            .into_iter()
            .any(|c| {
                dbg!(&c.names);
                //When we receive the container names from Docker, they all start with a `/` for some reason.
                c.names.into_iter().any(|s| s.ends_with(&container_name))
            });
        dbg!(&container_exists);
        if !container_exists {
            //No container has been created yet, build it from scratch
            debug!("Creating new container {}", container_name);
            let redis = &crate::CONFIG.redis.address;
            //For Redis to succeed in connecting the format of the address field must be <host>:<port>
            let split = redis.find(':').unwrap();
            let redis_host = &redis[..split];
            let redis_port = &redis[split + 1..];

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
            let options = CreateContainerOptions {
                name: &container_name,
            };
            //Print any warnings
            let result = docker.create_container(Some(options), config).await?;
            debug!(
                "Successfully created container with name {}:{}",
                container_name, result.id
            );
            let id = &result.id;
            if let Some(w) = result.warnings {
                w.into_iter().for_each(|w| warn!("Container {}: {}", id, w));
            }
        }

        //Fire this sucker up~
        docker
            .start_container(&container_name, None::<StartContainerOptions<String>>)
            .await?;

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
) -> Result<Status, BackendError> {
    //If the module doesn't exist, 404
    let module = ModuleInfo { name, version };
    if !module_exists(&docker, &module).await? {
        warn!("Couln't find module {}", module);
        Ok(Status::NotFound)
    } else {
        //If the module isn't running, don't bother stopping it
        if !module_is_running(&docker, &module).await? {
            Ok(Status::NotModified)
        } else {
            let options = StopContainerOptions { t: 60 };
            let container = module.to_string().replace(":", "-");
            match docker.stop_container(&container, Some(options)).await {
                Ok(_) => {
                    info!("Module {} stopped by {}", container, session.username);
                    Ok(Status::NoContent)
                }
                Err(e) => {
                    error!(
                        "Failed attempt to stop {} by {}: {:?}",
                        container, session.username, e
                    );
                    Err(BackendError::Docker(e))
                }
            }
        }
    }
}
