use actix_web::web;
use utoipa::OpenApi;
use utoipa::{
    Modify,
    openapi::security::{Http, HttpAuthScheme, SecurityScheme},
};
use utoipa_swagger_ui::SwaggerUi;

use crate::entities::{
    CodeType, MemberType, MonthlyCardPlanType, MonthlyCardStatus, RechargeStatus,
};
use crate::handlers;
use crate::handlers::recharge::UnifiedConfirmRequest;
use crate::models::*;

struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.as_mut().unwrap();
        components.add_security_scheme(
            "bearer_auth",
            SecurityScheme::Http(Http::new(HttpAuthScheme::Bearer)),
        )
    }
}

#[derive(OpenApi)]
#[openapi(
    paths(
        handlers::auth::send_code,
        handlers::auth::register,
        handlers::auth::login,
        handlers::auth::refresh,
        handlers::auth::reset_password,
        handlers::user::get_profile,
        handlers::user::update_profile,
        handlers::user::get_referrals,
        handlers::user::get_wallet_transactions,
        handlers::order::get_orders,
        handlers::discount_code::get_discount_codes,
        handlers::discount_code::redeem_discount_code,
        handlers::discount_code::redeem_balance_discount_code,
        handlers::recharge::create_payment_intent,
        handlers::recharge::confirm_recharge,
        handlers::recharge::get_history,
        handlers::recharge::create_membership_payment_intent,
        handlers::recharge::confirm_membership,
        handlers::recharge::create_monthly_card_payment_intent,
        handlers::recharge::confirm_monthly_card,
        handlers::recharge::confirm_unified,
        handlers::lucky_draw::get_chances,
        handlers::lucky_draw::get_prizes,
        handlers::lucky_draw::get_records,
        handlers::lucky_draw::spin,
        handlers::email::send_contact_email,
        handlers::email::subscribe,
    ),
    components(
        schemas(
            UserResponse,
            UserStatistics,
            CreateUserRequest,
            LoginRequest,
            UpdateUserRequest,
            AuthResponse,
            SendCodeRequest,
            SendCodeResponse,
            ResetPasswordRequest,
            MemberType,
            OrderResponse,
            OrderQuery,
            DiscountCodeResponse,
            DiscountCodeQuery,
            RedeemDiscountCodeRequest,
            RedeemDiscountCodeResponse,
            RedeemBalanceDiscountCodeRequest,
            RedeemBalanceDiscountCodeResponse,
            CodeType,
            RechargeRecordResponse,
            CreatePaymentIntentRequest,
            CreatePaymentIntentResponse,
            ConfirmRechargeRequest,
            ConfirmRechargeResponse,
            RechargeQuery,
            RechargeStatus,
            WalletTransactionKind,
            WalletTransactionResponse,
            MembershipPurchaseRecordResponse,
            CreateMembershipIntentRequest,
            CreateMembershipIntentResponse,
            ConfirmMembershipRequest,
            ConfirmMembershipResponse,
            ApiError,
            MonthlyCardPlanType,
            MonthlyCardStatus,
            CreateMonthlyCardIntentRequest,
            CreateMonthlyCardIntentResponse,
            ConfirmMonthlyCardRequest,
            ConfirmMonthlyCardResponse,
            UnifiedConfirmRequest,
            PaginatedOrderResponse,
            AuthApiResponse,
            SendCodeApiResponse,
            UserApiResponse,
            OrderListApiResponse,
            LuckyDrawChancesResponse,
            LuckyDrawPrizeResponse,
            LuckyDrawRecordResponse,
            LuckyDrawRecordQuery,
            LuckyDrawSpinResponse,
            SendContactEmailRequest,
            SubscribeRequest,
        )
    ),
    modifiers(&SecurityAddon),
    tags(
        (name = "auth", description = "Authentication API"),
        (name = "user", description = "User management API"),
        (name = "order", description = "Order management API"),
        (name = "discount", description = "Discount code API"),
        (name = "recharge", description = "Recharge API"),
    (name = "membership", description = "Membership purchase API"),
    (name = "monthly_card", description = "Monthly card API"),
    (name = "payments", description = "Unified payments API"),
    (name = "lucky_draw", description = "Lucky draw wheel API"),
    (name = "email", description = "Email API"),
    ),
    info(
        title = "KKSS Backend API",
        version = "1.0.0",
        description = "KKSS Backend REST API documentation",
        contact(
            name = "API Support",
            email = "driftcell@icloud.com"
        )
    ),
    servers(
        (url = "/api/v1", description = "Local server")
    )
)]
pub struct ApiDoc;

pub fn swagger_config(cfg: &mut web::ServiceConfig) {
    cfg.service(
        SwaggerUi::new("/swagger-ui/{_:.*}").url("/api-docs/openapi.json", ApiDoc::openapi()),
    )
    .route(
        "/swagger-ui",
        web::get().to(|| async {
            actix_web::HttpResponse::Found()
                .append_header(("Location", "/swagger-ui/"))
                .finish()
        }),
    );
}
