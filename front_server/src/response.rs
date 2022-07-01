use actix_web::{HttpResponse};
use serde::{Serialize};

#[derive(Serialize)]
pub struct MyResponse {
  pub code: i32,
  pub message: String,
}

impl MyResponse {

  pub fn ok() -> HttpResponse {
    HttpResponse::Ok().json(
      MyResponse{
        code: 2000, 
        message: "Success".to_string()
      }
    )
  }

  pub fn bad_request(e: &str) -> HttpResponse {
    HttpResponse::BadRequest().json(
      MyResponse{
        code: 4000, 
        message: e.to_string()
      }
    )
  }

  pub fn unauthorized(e: &str) -> HttpResponse {
    HttpResponse::Unauthorized().json(
      MyResponse{
        code: 4001, 
        message: e.to_string()
      }
    )
  }

  pub fn not_found(e: &str) -> HttpResponse {
    HttpResponse::NotFound().json(
      MyResponse{
        code: 4004, 
        message: e.to_string()
      }
    )
  }

  pub fn internal_server_error(e: &str) -> HttpResponse {
    HttpResponse::InternalServerError().json(
      MyResponse{
        code: 5000, 
        message: e.to_string()
      }
    )
  }

  pub fn item_not_found() -> HttpResponse {
    MyResponse::not_found("Item not found")
  }

  pub fn incorrect_password() -> HttpResponse {
    MyResponse::unauthorized("Incorrect password")
  }

}
