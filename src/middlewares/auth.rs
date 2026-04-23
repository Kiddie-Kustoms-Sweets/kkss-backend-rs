use crate::error::AppError;
use crate::utils::JwtService;
use actix_web::http::Method;
use actix_web::{
    Error, HttpMessage,
    dev::{Service, ServiceRequest, ServiceResponse, Transform, forward_ready},
};
use futures_util::future::LocalBoxFuture;
use std::future::{Ready, ready};

// 公开路径配置
struct PublicPaths {
    exact_paths: Vec<&'static str>,
    prefix_paths: Vec<&'static str>,
    excluded_paths: Vec<&'static str>,
}

impl PublicPaths {
    fn new() -> Self {
        Self {
            // 完全匹配的公开路径
            exact_paths: vec!["/swagger-ui", "/swagger-ui/", "/api-docs/openapi.json"],
            // 前缀匹配的公开路径
            prefix_paths: vec!["/swagger-ui/", "/api-docs/", "/api/v1/auth/", "/webhook/", "/api/v1/email/"],
            // 需要排除的路径（即使在公开前缀下也需要认证）
            excluded_paths: vec!["/api/v1/auth/refresh"],
        }
    }

    fn is_public_path(&self, path: &str) -> bool {
        // 首先检查是否在排除列表中
        if self
            .excluded_paths
            .iter()
            .any(|&excluded| path.starts_with(excluded))
        {
            return false;
        }

        // 检查完全匹配
        if self.exact_paths.contains(&path) {
            return true;
        }

        // 检查前缀匹配
        self.prefix_paths
            .iter()
            .any(|&prefix| path.starts_with(prefix))
    }
}

pub struct AuthMiddleware {
    jwt_service: JwtService,
}

impl AuthMiddleware {
    pub fn new(jwt_service: JwtService) -> Self {
        Self { jwt_service }
    }
}

impl<S, B> Transform<S, ServiceRequest> for AuthMiddleware
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type InitError = ();
    type Transform = AuthMiddlewareService<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(AuthMiddlewareService {
            service,
            jwt_service: self.jwt_service.clone(),
            public_paths: PublicPaths::new(),
        }))
    }
}

pub struct AuthMiddlewareService<S> {
    service: S,
    jwt_service: JwtService,
    public_paths: PublicPaths,
}

impl<S, B> Service<ServiceRequest> for AuthMiddlewareService<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
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

        // 检查是否为公开路径
        let path = req.path();

        if self.public_paths.is_public_path(path) {
            let fut = self.service.call(req);
            return Box::pin(fut);
        }

        // 提取Authorization header
        let auth_header = req.headers().get("Authorization");

        let token = if let Some(auth_value) = auth_header {
            if let Ok(auth_str) = auth_value.to_str() {
                auth_str.strip_prefix("Bearer ")
            } else {
                None
            }
        } else {
            None
        };

        let jwt_service = self.jwt_service.clone();

        if let Some(token) = token {
            match jwt_service.verify_access_token(token) {
                Ok(claims) => {
                    // 将用户ID添加到请求扩展中
                    req.extensions_mut()
                        .insert(claims.sub.parse::<i64>().unwrap_or(0));
                    let fut = self.service.call(req);
                    Box::pin(fut)
                }
                Err(_) => {
                    let error = AppError::AuthError("Invalid access token".to_string());
                    Box::pin(async move { Err(error.into()) })
                }
            }
        } else {
            let error = AppError::AuthError("Missing access token".to_string());
            Box::pin(async move { Err(error.into()) })
        }
    }
}

/// 用于获取当前用户ID的辅助函数
pub fn get_current_user_id(req: &ServiceRequest) -> Option<i64> {
    req.extensions().get::<i64>().copied()
}
