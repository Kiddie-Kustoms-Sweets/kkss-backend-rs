use crate::models::*;
use crate::services::AdminService;
use actix_web::{HttpResponse, ResponseError, Result, web};
use serde_json::json;

fn ok_json<T: serde::Serialize>(data: T) -> Result<HttpResponse> {
    Ok(HttpResponse::Ok().json(json!({
        "success": true,
        "data": data
    })))
}

pub async fn admin_login(
    admin_service: web::Data<AdminService>,
    request: web::Json<AdminLoginRequest>,
) -> Result<HttpResponse> {
    match admin_service.login(request.into_inner()).await {
        Ok(resp) => ok_json(resp),
        Err(e) => Ok(e.error_response()),
    }
}

pub async fn get_overview(admin_service: web::Data<AdminService>) -> Result<HttpResponse> {
    match admin_service.get_overview().await {
        Ok(resp) => ok_json(resp),
        Err(e) => Ok(e.error_response()),
    }
}

pub async fn get_coupon_stats(admin_service: web::Data<AdminService>) -> Result<HttpResponse> {
    match admin_service.get_coupon_stats().await {
        Ok(resp) => ok_json(resp),
        Err(e) => Ok(e.error_response()),
    }
}

pub async fn list_users(
    admin_service: web::Data<AdminService>,
    query: web::Query<AdminUserQuery>,
) -> Result<HttpResponse> {
    match admin_service.list_users(&query).await {
        Ok(resp) => ok_json(resp),
        Err(e) => Ok(e.error_response()),
    }
}

pub async fn get_user_detail(
    admin_service: web::Data<AdminService>,
    path: web::Path<i64>,
) -> Result<HttpResponse> {
    match admin_service.get_user_detail(path.into_inner()).await {
        Ok(resp) => ok_json(resp),
        Err(e) => Ok(e.error_response()),
    }
}

pub async fn get_user_referrals(
    admin_service: web::Data<AdminService>,
    path: web::Path<i64>,
    query: web::Query<AdminPageQuery>,
) -> Result<HttpResponse> {
    match admin_service
        .get_user_referrals(path.into_inner(), &query)
        .await
    {
        Ok(resp) => ok_json(resp),
        Err(e) => Ok(e.error_response()),
    }
}

pub async fn adjust_balance(
    admin_service: web::Data<AdminService>,
    path: web::Path<i64>,
    request: web::Json<AdjustBalanceRequest>,
) -> Result<HttpResponse> {
    match admin_service
        .adjust_balance(path.into_inner(), request.into_inner())
        .await
    {
        Ok(resp) => ok_json(resp),
        Err(e) => Ok(e.error_response()),
    }
}

pub async fn adjust_stamps(
    admin_service: web::Data<AdminService>,
    path: web::Path<i64>,
    request: web::Json<AdjustStampsRequest>,
) -> Result<HttpResponse> {
    match admin_service
        .adjust_stamps(path.into_inner(), request.into_inner())
        .await
    {
        Ok(resp) => ok_json(resp),
        Err(e) => Ok(e.error_response()),
    }
}

pub async fn grant_coupon(
    admin_service: web::Data<AdminService>,
    path: web::Path<i64>,
    request: web::Json<AdminGrantCouponRequest>,
) -> Result<HttpResponse> {
    match admin_service
        .grant_coupon(path.into_inner(), request.into_inner())
        .await
    {
        Ok(resp) => ok_json(resp),
        Err(e) => Ok(e.error_response()),
    }
}

pub async fn list_coupons(
    admin_service: web::Data<AdminService>,
    query: web::Query<AdminCouponQuery>,
) -> Result<HttpResponse> {
    match admin_service.list_coupons(&query).await {
        Ok(resp) => ok_json(resp),
        Err(e) => Ok(e.error_response()),
    }
}

pub async fn revoke_coupon(
    admin_service: web::Data<AdminService>,
    path: web::Path<i64>,
) -> Result<HttpResponse> {
    match admin_service.revoke_coupon(path.into_inner()).await {
        Ok(_) => ok_json(json!({ "revoked": true })),
        Err(e) => Ok(e.error_response()),
    }
}

pub fn admin_config(cfg: &mut web::ServiceConfig) {
    cfg.route("/auth/login", web::post().to(admin_login))
        .route("/stats/overview", web::get().to(get_overview))
        .route("/stats/coupons", web::get().to(get_coupon_stats))
        .route("/users", web::get().to(list_users))
        .route("/users/{id}", web::get().to(get_user_detail))
        .route("/users/{id}/referrals", web::get().to(get_user_referrals))
        .route("/users/{id}/adjust-balance", web::post().to(adjust_balance))
        .route("/users/{id}/adjust-stamps", web::post().to(adjust_stamps))
        .route("/users/{id}/grant-coupon", web::post().to(grant_coupon))
        .route("/coupons", web::get().to(list_coupons))
        .route("/coupons/{id}", web::delete().to(revoke_coupon));
}
