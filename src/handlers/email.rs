use crate::external::EmailService;
use crate::models::email::{SendContactEmailRequest, SubscribeRequest};
use actix_web::{HttpResponse, ResponseError, Result, web};
use serde_json::json;

#[utoipa::path(
    post,
    path = "/email/contact",
    tag = "email",
    request_body = SendContactEmailRequest,
    responses(
        (status = 200, description = "邮件发送成功"),
        (status = 400, description = "请求参数错误"),
        (status = 500, description = "邮件发送失败")
    )
)]
pub async fn send_contact_email(
    email_service: web::Data<EmailService>,
    request: web::Json<SendContactEmailRequest>,
) -> Result<HttpResponse> {
    let req = request.into_inner();

    if req.email.is_empty()
        || req.firstname.is_empty()
        || req.lastname.is_empty()
        || req.content.is_empty()
    {
        return Ok(HttpResponse::BadRequest().json(json!({
            "success": false,
            "error": {
                "code": "VALIDATION_ERROR",
                "message": "email, firstname, lastname and content are required"
            }
        })));
    }

    match email_service
        .send_contact_email(
            &req.email,
            &req.firstname,
            &req.lastname,
            &req.phone,
            &req.content,
        )
        .await
    {
        Ok(()) => Ok(HttpResponse::Ok().json(json!({
            "success": true,
            "data": null,
            "message": "Email sent successfully"
        }))),
        Err(e) => Ok(e.error_response()),
    }
}

#[utoipa::path(
    post,
    path = "/email/subscribe",
    tag = "email",
    request_body = SubscribeRequest,
    responses(
        (status = 200, description = "订阅邮件发送成功"),
        (status = 400, description = "请求参数错误"),
        (status = 500, description = "邮件发送失败")
    )
)]
pub async fn subscribe(
    email_service: web::Data<EmailService>,
    request: web::Json<SubscribeRequest>,
) -> Result<HttpResponse> {
    let req = request.into_inner();

    if req.email.is_empty() {
        return Ok(HttpResponse::BadRequest().json(json!({
            "success": false,
            "error": {
                "code": "VALIDATION_ERROR",
                "message": "email is required"
            }
        })));
    }

    match email_service.send_subscribe_email(&req.email).await {
        Ok(()) => Ok(HttpResponse::Ok().json(json!({
            "success": true,
            "data": null,
            "message": "Subscription email sent successfully"
        }))),
        Err(e) => Ok(e.error_response()),
    }
}

pub fn email_config(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/email")
            .route("/contact", web::post().to(send_contact_email))
            .route("/subscribe", web::post().to(subscribe)),
    );
}
