use rocket::response::{NamedFile, Redirect};
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
pub mod test;

//Admin index with session: Show the page
#[get("/admin")]
pub async fn index(_session: AdminSession) -> Option<NamedFile> {
    NamedFile::open("dist/admin.html").ok()
}
//Without the session: redirect to the login page
#[get("/admin", rank = 2)]
pub async fn index_no_session() -> Redirect {
    Redirect::to(uri!(login_index))
}

#[get("/admin.js")]
pub fn index_js() -> Option<NamedFile> {
    NamedFile::open("dist/admin.js").ok()
}

#[get("/admin/me")]
pub async fn get_me(session: AdminSession) -> Json<AdminSession> {
    Json(session)
}
