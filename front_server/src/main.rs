use actix_web::{web, error, App, HttpServer, HttpResponse};
mod model;
mod utils;
mod response;
mod url_handler;
mod db_handler;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let dev_names = utils::get_dev_names();
    let accessor = db_handler::DataAccessor::new(&dev_names).await;
    let accessor_state = web::Data::new(accessor);
    let server = HttpServer::new(move || {
        App::new()
            .app_data(accessor_state.clone())
            .service(url_handler::get_freqdata)
            .app_data(
                web::JsonConfig::default().error_handler(
                    |err, _req| {
                        error::InternalError::from_response(
                            "", 
                            HttpResponse::BadRequest()
                                .content_type("application/json")
                                .body(format!(r#"{{"error":"{}"}}"#, err)),
                        ).into()
                    }
                )
            )
    })
    .bind(utils::get_url())?;
    server.run().await
}
