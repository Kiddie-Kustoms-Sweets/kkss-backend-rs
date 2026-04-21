use actix_web::{App, HttpServer, middleware::Logger, web};
use chrono::Local; // timestamp in log lines
use env_logger::{Env, Target};
use std::io::Write; // for env_logger custom formatter
use std::sync::Arc;
use tokio::sync::Mutex;

use kkss_backend::tasks;
use kkss_backend::{
    config::Config,
    database::{create_pool, run_migrations},
    external::{EmailService, SevenCloudAPI, StripeService, TwilioService},
    handlers,
    middlewares::{AuthMiddleware, create_cors},
    services::*,
    swagger::swagger_config,
    utils::JwtService,
};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info"))
        .format(|buf, record| {
            let ts = Local::now().format("%Y-%m-%dT%H:%M:%S%.3f%:z");
            let level = record.level().as_str().to_ascii_lowercase();
            let msg_json = serde_json::to_string(&format!("{}", record.args()))
                .unwrap_or_else(|_| "\"<invalid utf8>\"".to_string());
            writeln!(
                buf,
                "{{\"timestamp\":\"{}\",\"level\":\"{}\",\"message\":{},\"target\":\"{}\"}}",
                ts,
                level,
                msg_json,
                record.target(),
            )
        })
        .target(Target::Stdout)
        .init();

    // 加载配置
    let config = Config::from_toml().expect("Failed to load configuration file");

    // 创建数据库连接池
    let pool = create_pool(&config.database)
        .await
        .expect("Failed to create database connection pool");

    // 运行数据库迁移
    run_migrations(&pool)
        .await
        .expect("Failed to run database migrations");

    // 创建JWT服务
    let jwt_service = JwtService::new(
        &config.jwt.secret,
        config.jwt.access_token_expires_in,
        config.jwt.refresh_token_expires_in,
    );

    // 创建外部服务
    let twilio_service = TwilioService::new(config.twilio.clone());
    let turnstile_service = kkss_backend::external::TurnstileService::new(config.turnstile.clone());
    let stripe_service = StripeService::new(config.stripe.clone());
    let email_service = EmailService::new(config.smtp.clone());

    let mut sevencloud_api = SevenCloudAPI::new(config.sevencloud.clone());
    if let Err(e) = sevencloud_api.login().await {
        log::error!("SevenCloud API login failed: {e:?}");
    }
    let sevencloud_api = Arc::new(Mutex::new(sevencloud_api));

    // 创建服务 (注意顺序: 先创建依赖，再注入)
    let discount_code_service = DiscountCodeService::new(pool.clone(), sevencloud_api.clone());
    let auth_service = AuthService::new(
        pool.clone(),
        jwt_service.clone(),
        twilio_service,
        discount_code_service.clone(),
    );
    let user_service = UserService::new(pool.clone());
    let order_service = OrderService::new(pool.clone());
    let recharge_service = RechargeService::new(pool.clone(), stripe_service.clone());
    let membership_service = MembershipService::new(
        pool.clone(),
        stripe_service.clone(),
        discount_code_service.clone(),
    );
    let monthly_card_service = MonthlyCardService::new(
        pool.clone(),
        stripe_service.clone(),
        discount_code_service.clone(),
    );
    let stripe_transaction_service = StripeTransactionService::new(pool.clone());
    let sync_service = SyncService::new(pool.clone(), sevencloud_api.clone());
    let birthday_reward_service = BirthdayRewardService::new(pool.clone());
    let lucky_draw_service = LuckyDrawService::new(pool.clone(), discount_code_service.clone());

    // 启动后台定时任务
    tasks::spawn_all(
        sync_service.clone(),
        membership_service.clone(),
        birthday_reward_service.clone(),
        monthly_card_service.clone(),
    );

    // 启动HTTP服务器
    log::info!(
        "Starting HTTP server at {}:{}",
        config.server.host,
        config.server.port
    );

    HttpServer::new(move || {
        App::new()
            .wrap(Logger::default())
            .wrap(create_cors())
            .wrap(AuthMiddleware::new(jwt_service.clone()))
            .app_data(web::Data::new(auth_service.clone()))
            .app_data(web::Data::new(turnstile_service.clone()))
            .app_data(web::Data::new(user_service.clone()))
            .app_data(web::Data::new(order_service.clone()))
            .app_data(web::Data::new(discount_code_service.clone()))
            .app_data(web::Data::new(recharge_service.clone()))
            .app_data(web::Data::new(membership_service.clone()))
            .app_data(web::Data::new(monthly_card_service.clone()))
            .app_data(web::Data::new(birthday_reward_service.clone()))
            .app_data(web::Data::new(stripe_transaction_service.clone()))
            .app_data(web::Data::new(stripe_service.clone()))
            .app_data(web::Data::new(sync_service.clone()))
            .app_data(web::Data::new(lucky_draw_service.clone()))
            .app_data(web::Data::new(email_service.clone()))
            .configure(swagger_config)
            .configure(handlers::webhook_config)
            .service(
                web::scope("/api/v1")
                    .configure(handlers::auth_config)
                    .configure(handlers::user_config)
                    .configure(handlers::order_config)
                    .configure(handlers::discount_code_config)
                    .configure(handlers::recharge_config)
                    .configure(handlers::membership_config)
                    .configure(handlers::lucky_draw_config)
                    .configure(handlers::email_config)
                    .configure(|cfg| {
                        handlers::recharge::monthly_card_config(cfg);
                    })
                    .route(
                        "/payments/confirm",
                        web::post().to(handlers::recharge::confirm_unified),
                    ),
            )
    })
    .bind((config.server.host.as_str(), config.server.port))?
    .run()
    .await
}
