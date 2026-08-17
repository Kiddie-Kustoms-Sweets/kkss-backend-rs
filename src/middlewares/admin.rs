use crate::error::AppError;
use crate::utils::JwtService;
use actix_web::http::Method;
use actix_web::{
    Error, HttpMessage,
    dev::{Service, ServiceRequest, ServiceResponse, Transform, forward_ready},
};
use futures_util::future::LocalBoxFuture;
use std::future::{Ready, ready};

/// 无需 admin token 即可访问的管理端点
const PUBLIC_ADMIN_PATHS: &[&str] = &["/api/v1/admin/auth/login"];

/// 管理员认证中间件
///
/// 挂载在 `/api/v1/admin` 作用域上（全局 AuthMiddleware 已对该前缀放行）。
/// 只接受 token_type = "admin" 的 JWT；普通用户 token 无法访问管理端点。
pub struct AdminMiddleware {
    jwt_service: JwtService,
}

impl AdminMiddleware {
    pub fn new(jwt_service: JwtService) -> Self {
        Self { jwt_service }
    }
}

impl<S, B> Transform<S, ServiceRequest> for AdminMiddleware
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type InitError = ();
    type Transform = AdminMiddlewareService<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(AdminMiddlewareService {
            service,
            jwt_service: self.jwt_service.clone(),
        }))
    }
}

pub struct AdminMiddlewareService<S> {
    service: S,
    jwt_service: JwtService,
}

impl<S, B> Service<ServiceRequest> for AdminMiddlewareService<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        // 放行所有 CORS 预检请求
        if req.method() == Method::OPTIONS {
            let fut = self.service.call(req);
            return Box::pin(fut);
        }

        // 放行 admin 登录等公开端点
        if PUBLIC_ADMIN_PATHS.contains(&req.path()) {
            let fut = self.service.call(req);
            return Box::pin(fut);
        }

        // 提取 Authorization header
        let token = req
            .headers()
            .get("Authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.strip_prefix("Bearer "))
            .map(|s| s.to_string());

        let jwt_service = self.jwt_service.clone();

        if let Some(token) = token {
            match jwt_service.verify_admin_token(&token) {
                Ok(claims) => {
                    // 将管理员用户名添加到请求扩展中
                    req.extensions_mut().insert(claims.sub);
                    let fut = self.service.call(req);
                    Box::pin(fut)
                }
                Err(_) => {
                    let error = AppError::AuthError("Invalid admin token".to_string());
                    Box::pin(async move { Err(error.into()) })
                }
            }
        } else {
            let error = AppError::AuthError("Missing admin token".to_string());
            Box::pin(async move { Err(error.into()) })
        }
    }
}
