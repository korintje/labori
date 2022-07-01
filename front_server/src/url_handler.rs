use actix_web::{get, post, delete, put, web, HttpResponse, Responder};
use crate::db_handler::{DataAccessor};
use crate::{utils, response};
use response::{MyResponse};
use crate::model::{Filter, FreqFilter};

#[get("/{dev_id}/{table_name}/data")]
async fn get_freqdata(
  path: web::Path<(u16, String)>,
  filter: web::Query<FreqFilter>,
  accessor: web::Data<DataAccessor>
) -> impl Responder {
  let (dev_id, table_name) = path.into_inner();
  let filter = filter.into_inner();
  let result = accessor.get_freqdata(dev_id, &table_name, filter.from).await;
  match result {
    Err(_) => MyResponse::item_not_found(),
    Ok(item) => HttpResponse::Ok().json(item),
  }
}

