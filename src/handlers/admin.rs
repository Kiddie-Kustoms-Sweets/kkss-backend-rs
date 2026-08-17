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

#[utoipa::path(
    post,
    path = "/admin/auth/login",
    tag = "admin",
    request_body = AdminLoginRequest,
    responses(
        (status = 200, description = "管理员登录成功", body = AdminLoginResponse),
        (status = 401, description = "凭证错误"),
        (status = 500, description = "管理员账号未配置")
    )
)]
pub async fn admin_login(
    admin_service: web::Data<AdminService>,
    request: web::Json<AdminLoginRequest>,
) -> Result<HttpResponse> {
    match admin_service.login(request.into_inner()).await {
        Ok(resp) => ok_json(resp),
        Err(e) => Ok(e.error_response()),
    }
}

#[utoipa::path(
    get,
    path = "/admin/stats/overview",
    tag = "admin",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "获取总览统计成功", body = AdminOverviewStats),
        (status = 401, description = "未授权")
    )
)]
pub async fn get_overview(admin_service: web::Data<AdminService>) -> Result<HttpResponse> {
    match admin_service.get_overview().await {
        Ok(resp) => ok_json(resp),
        Err(e) => Ok(e.error_response()),
    }
}

#[utoipa::path(
    get,
    path = "/admin/stats/coupons",
    tag = "admin",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "获取优惠券统计成功", body = Vec<AdminCouponStatItem>),
        (status = 401, description = "未授权")
    )
)]
pub async fn get_coupon_stats(admin_service: web::Data<AdminService>) -> Result<HttpResponse> {
    match admin_service.get_coupon_stats().await {
        Ok(resp) => ok_json(resp),
        Err(e) => Ok(e.error_response()),
    }
}

#[utoipa::path(
    get,
    path = "/admin/users",
    tag = "admin",
    params(AdminUserQuery),
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "获取用户列表成功", body = PaginatedResponse<UserResponse>),
        (status = 400, description = "请求参数错误"),
        (status = 401, description = "未授权")
    )
)]
pub async fn list_users(
    admin_service: web::Data<AdminService>,
    query: web::Query<AdminUserQuery>,
) -> Result<HttpResponse> {
    match admin_service.list_users(&query).await {
        Ok(resp) => ok_json(resp),
        Err(e) => Ok(e.error_response()),
    }
}

#[utoipa::path(
    get,
    path = "/admin/users/{id}",
    tag = "admin",
    params(("id" = i64, Path, description = "用户ID")),
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "获取用户详情成功", body = AdminUserDetailResponse),
        (status = 401, description = "未授权"),
        (status = 404, description = "用户不存在")
    )
)]
pub async fn get_user_detail(
    admin_service: web::Data<AdminService>,
    path: web::Path<i64>,
) -> Result<HttpResponse> {
    match admin_service.get_user_detail(path.into_inner()).await {
        Ok(resp) => ok_json(resp),
        Err(e) => Ok(e.error_response()),
    }
}

#[utoipa::path(
    get,
    path = "/admin/users/{id}/referrals",
    tag = "admin",
    params(
        ("id" = i64, Path, description = "用户ID"),
        AdminPageQuery
    ),
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "获取邀请列表成功", body = PaginatedResponse<UserResponse>),
        (status = 401, description = "未授权")
    )
)]
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

#[utoipa::path(
    post,
    path = "/admin/users/{id}/adjust-balance",
    tag = "admin",
    params(("id" = i64, Path, description = "用户ID")),
    request_body = AdjustBalanceRequest,
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "余额调整成功", body = UserResponse),
        (status = 400, description = "请求参数错误（如调整后余额为负）"),
        (status = 401, description = "未授权"),
        (status = 404, description = "用户不存在")
    )
)]
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

#[utoipa::path(
    post,
    path = "/admin/users/{id}/adjust-stamps",
    tag = "admin",
    params(("id" = i64, Path, description = "用户ID")),
    request_body = AdjustStampsRequest,
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "印花调整成功", body = UserResponse),
        (status = 400, description = "请求参数错误（如调整后印花为负）"),
        (status = 401, description = "未授权"),
        (status = 404, description = "用户不存在")
    )
)]
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

#[utoipa::path(
    post,
    path = "/admin/users/{id}/grant-coupon",
    tag = "admin",
    params(("id" = i64, Path, description = "用户ID")),
    request_body = AdminGrantCouponRequest,
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "发放优惠券成功", body = DiscountCodeResponse),
        (status = 400, description = "请求参数错误"),
        (status = 401, description = "未授权"),
        (status = 404, description = "用户不存在")
    )
)]
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

#[utoipa::path(
    get,
    path = "/admin/coupons",
    tag = "admin",
    params(AdminCouponQuery),
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "获取优惠券列表成功", body = PaginatedResponse<AdminCouponResponse>),
        (status = 400, description = "请求参数错误"),
        (status = 401, description = "未授权")
    )
)]
pub async fn list_coupons(
    admin_service: web::Data<AdminService>,
    query: web::Query<AdminCouponQuery>,
) -> Result<HttpResponse> {
    match admin_service.list_coupons(&query).await {
        Ok(resp) => ok_json(resp),
        Err(e) => Ok(e.error_response()),
    }
}

#[utoipa::path(
    delete,
    path = "/admin/coupons/{id}",
    tag = "admin",
    params(("id" = i64, Path, description = "优惠券ID")),
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "撤销优惠券成功（同步删除 SevenCloud 记录）"),
        (status = 401, description = "未授权"),
        (status = 404, description = "优惠券不存在"),
        (status = 502, description = "SevenCloud 删除失败，本地记录已保留")
    )
)]
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
