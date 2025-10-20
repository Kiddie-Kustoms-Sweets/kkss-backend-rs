# KKSS Backend

基于Rust actix-web框架的冰淇凌推广网站后端系统，主要为消费者提供会员管理、优惠码兑换、充值等功能的API服务。

## 快速开始

### 1. 环境准备

确保已安装 Rust 1.89+:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### 2. 项目设置

```bash
# 克隆项目
git clone https://github.com/Kiddie-Kustoms-Sweets/kkss-backend-rs.git
cd kkss-backend-rs
```

### 3. 运行项目

```bash
# 开发模式
cargo run

# 或者使用 cargo watch 自动重启
cargo install cargo-watch
cargo watch -x run
```

服务器将在 `http://localhost:8080` 启动。

## 配置说明

可以通过两种方式提供配置：

1. 使用 `config.toml`（参考 `config.toml.example`），然后可用环境变量覆盖其中的值。
2. 完全不提供 `config.toml`，所有值通过环境变量注入（此时必须至少提供 `DATABASE_URL`）。

当 `CONFIG_PATH`（默认 `config.toml`）指向的文件不存在时，程序会使用一套缺省值 + 环境变量构建配置。

配置文件格式（可选）：

```toml
[server]
host = "0.0.0.0"
port = 8080

[database]
url = "postgres://postgres:postgres@localhost:5432/kkss"
max_connections = 10

[jwt]
secret = "your-jwt-secret"
access_token_expires_in = 7200
refresh_token_expires_in = 2592000

[twilio]
account_sid = "your-twilio-account-sid"
auth_token = "your-twilio-auth-token"
from_phone = "+1234567890"
verify_service_sid = "VAxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"

[stripe]
secret_key = "sk_test_your-stripe-secret-key"
webhook_secret = "whsec_your-webhook-secret"

[sevencloud]
username = "your-sevencloud-username"
password = "your-sevencloud-password"
base_url = "https://sz.sunzee.com.cn"
```

### 环境变量

支持的环境变量（全部大写），文件存在时用于覆盖；文件不存在时用于构建：

- 基础：
  - `CONFIG_PATH` 指定配置文件路径（可选）
- 服务：
  - `SERVER_HOST` (默认 `0.0.0.0`)
  - `SERVER_PORT` (默认 `8080`)
- 数据库：
  - `DATABASE_URL` (无文件模式下必填)
  - `DB_MAX_CONNECTIONS` (默认 `10`)
- JWT：
  - `JWT_SECRET` (默认 `change-me-in-production`)
  - `JWT_ACCESS_EXPIRES_IN` (默认 `7200` 秒)
  - `JWT_REFRESH_EXPIRES_IN` (默认 `2592000` 秒)
- Twilio：
  - `TWILIO_ACCOUNT_SID`
  - `TWILIO_AUTH_TOKEN`
  - `TWILIO_FROM_PHONE`
  - `TWILIO_VERIFY_SERVICE_SID` (Twilio Verify 服务 SID，必需)
- Stripe：
  - `STRIPE_SECRET_KEY`
  - `STRIPE_WEBHOOK_SECRET`
- 七云：
  - `SEVENCLOUD_USERNAME`
  - `SEVENCLOUD_PASSWORD`
  - `SEVENCLOUD_BASE_URL` (默认 `https://sz.sunzee.com.cn`)

示例（纯环境变量运行）：

```bash
export DATABASE_URL="postgres://postgres:postgres@localhost:5432/kkss"
export JWT_SECRET="super-secret"
export STRIPE_SECRET_KEY="sk_test_xxx"
export STRIPE_WEBHOOK_SECRET="whsec_xxx"
export SERVER_PORT=8080
cargo run
```

## 数据库设计

### 主要表结构

- `users` - 用户表
- `orders` - 订单表
- `discount_codes` - 优惠码表
- `recharge_records` - 充值记录表
- `sweet_cash_transactions` - 甜品现金交易记录表

## Turnstile 保护短信验证码

后端支持 Cloudflare Turnstile 服务端校验。配置 `config.toml` 或环境变量：

- TURNSTILE_SECRET_KEY
- TURNSTILE_EXPECTED_HOSTNAME (可选)
- TURNSTILE_EXPECTED_ACTION (可选)

启用后，调用 `/api/v1/auth/send-code` 时需提供 `cf_turnstile_token` 字段（来自前端 Turnstile 小组件 `cf-turnstile-response`）。

## 开发

### 项目结构

```
src/
├── main.rs              # 主程序入口
├── lib.rs               # 库入口
├── config.rs            # 配置管理
├── error.rs             # 错误处理
├── database/            # 数据库连接
├── models/              # 数据模型
├── services/            # 业务逻辑
├── handlers/            # HTTP处理器
├── middlewares/         # 中间件
├── utils/               # 工具函数
└── external/            # 外部API集成
```

### 添加新功能

1. 在 `models/` 中定义数据模型
2. 在 `services/` 中实现业务逻辑
3. 在 `handlers/` 中添加HTTP处理器
4. 在 `main.rs` 中注册路由
