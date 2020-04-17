use rocket::response::NamedFile;
use rocket_contrib::json::Json;

mod adminsession;
use super::mime_consts;
use adminsession::AdminSession;

mod login;
mod map;
mod modules;

//Export all routes
pub use login::*;
pub use map::*;
pub use modules::*;

#[cfg(test)]
mod test;

#[get("/admin")]
pub async fn index(_session: AdminSession) -> Option<NamedFile> {
    NamedFile::open("dist/admin.html").ok()
}

#[get("/admin/me")]
pub async fn get_me(session: AdminSession) -> Json<AdminSession> {
    Json(session)
}
