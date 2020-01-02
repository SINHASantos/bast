use crate::models::{
    schema::{ghosts, pages, websites},
    SlimPage, Website,
};
use crate::utils::{to_client, UserError};
use crate::Db;
use actix_web::{web, HttpResponse};
use diesel::dsl::*;
use diesel::prelude::*;
use serde::Deserialize;

#[derive(Deserialize, Debug, Clone)]
pub struct Data {
    user_id: i32,
    website_id: i32,
    #[serde(rename(deserialize = "isNewSession"))]
    is_new_session: bool,
    href: String,
    hostname: String,
    origin: String,
    pathname: String,
    referrer: String,
}

pub async fn collect(
    params: Option<web::Query<Data>>,
    data: web::Data<Db>,
) -> Result<HttpResponse, UserError> {
    let result = web::block(move || -> Result<(), UserError> {
        let conn = &data.conn_pool()?;

        if params.is_none() {
            return Ok(());
        }

        let params = params.unwrap();

        let website: Website = update(websites::table)
            .filter(
                websites::id
                    .eq(&params.website_id)
                    .and(websites::user_id.eq(&params.user_id)),
            )
            .set((
                websites::visitors.eq(websites::visitors + 1),
                websites::sessions.eq(if params.is_new_session {
                    websites::sessions + 1
                } else {
                    websites::sessions + 0
                }),
            ))
            .get_result::<_>(conn)
            .map_err(|_| UserError::InternalServerError)?;

        // Upsert page.
        let upsert_page = SlimPage {
            website_id: website.id,
            pathname: params.pathname.clone(),
            visitors: 1,
            sessions: if params.is_new_session { 1 } else { 0 },
        };

        insert_into(pages::table)
            .values(upsert_page)
            .on_conflict(pages::pathname)
            .do_update()
            .set((
                pages::visitors.eq(pages::visitors + 1),
                pages::sessions.eq(if params.is_new_session {
                    pages::sessions + 1
                } else {
                    pages::sessions + 0
                }),
            ))
            .execute(&data.conn_pool()?)
            .map_err(|_| UserError::InternalServerError)?;

        // Store ghost.
        insert_into(ghosts::table)
            .values((
                ghosts::user_id.eq(website.user_id),
                ghosts::website_id.eq(website.id),
                ghosts::is_new_session.eq(params.is_new_session),
                ghosts::pathname.eq(params.pathname.clone()),
                ghosts::hostname.eq(params.hostname.clone()),
                ghosts::referrer.eq(if params.referrer.is_empty() {
                    None
                } else {
                    Some(params.referrer.clone())
                }),
            ))
            .execute(&data.conn_pool()?)
            .map_err(|_| UserError::InternalServerError)?;

        Ok(())
    })
    .await;

    to_client(result)
}
