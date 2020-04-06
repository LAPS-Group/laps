use crate::{types::BackendError, util};
use darkredis::ConnectionPool;
use rocket::{
    http::{Cookie, Status},
    request::{FromRequest, Outcome, Request},
    State,
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct AdminSession {
    pub username: String,
    pub is_super: bool,
}

#[rocket::async_trait]
impl<'a, 'r> FromRequest<'a, 'r> for AdminSession {
    type Error = BackendError;
    async fn from_request(request: &'a Request<'r>) -> Outcome<Self, Self::Error> {
        //Look for the session cookie
        let mut cookies = request.cookies();
        if let Some(token) = cookies.get_private("session-token") {
            //Verify that the session is valid
            let session_key = util::get_session_key(token.value());
            let pool = match request.guard::<State<'_, ConnectionPool>>().await {
                Outcome::Success(p) => p,
                //We always expect to be able to retrieve state.
                _ => panic!("Expected connectionpool state"),
            };
            let mut conn = pool.get().await;
            //Stored sessions are trusted inputs which should never be invalid JSON.
            match conn
                .get(&session_key)
                .await
                .map(|r| r.map(|o| serde_json::from_slice(&o)))
            {
                //All's good
                Ok(Some(Ok(session))) => Outcome::Success(session),
                //Failed to Deserialize session
                Ok(Some(Err(e))) => {
                    Outcome::Failure((Status::InternalServerError, BackendError::JsonError(e)))
                }
                //No session found, delete the cookie and forward
                Ok(None) => {
                    cookies.remove_private(Cookie::named("session-token"));
                    Outcome::Forward(())
                }
                //Redis Error
                Err(e) => Outcome::Failure((Status::InternalServerError, BackendError::Redis(e))),
            }
        } else {
            Outcome::Forward(())
        }
    }
}