use crate::config::SevenCloudConfig;
use crate::error::{AppError, AppResult};
use reqwest::Client;
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;
use std::time::Duration;

/// 指数退避间隔：最多重试 4 次（共 5 次尝试），总等待预算约 1 分钟，
/// 以覆盖七云服务端 1~2 分钟的抖动窗口。
const DEFAULT_RETRY_BACKOFF: [Duration; 4] = [
    Duration::from_secs(2),
    Duration::from_secs(5),
    Duration::from_secs(15),
    Duration::from_secs(30),
];

/// xlsx（ZIP）文件魔数。七云服务端抖动时会返回 200 + 空 xlsx
/// （Apache POI 创建了 workbook 但未填充数据的产物）。
const XLSX_MAGIC: [u8; 4] = [b'P', b'K', 0x03, 0x04];

fn deserialize_flexible_date<'de, D>(deserializer: D) -> Result<Option<i64>, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error;

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum FlexibleDate {
        Timestamp(i64),
        DateString(String),
    }

    match Option::<FlexibleDate>::deserialize(deserializer)? {
        None => Ok(None),
        Some(FlexibleDate::Timestamp(ts)) => Ok(Some(ts)),
        Some(FlexibleDate::DateString(s)) => {
            // Parse date string format "2025-10-17 10:34:22" to timestamp
            chrono::NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S")
                .map(|dt| Some(dt.and_utc().timestamp_millis()))
                .map_err(|e| Error::custom(format!("Failed to parse date string: {}", e)))
        }
    }
}

fn deserialize_flexible_date_required<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error;

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum FlexibleDate {
        Timestamp(i64),
        DateString(String),
    }

    match FlexibleDate::deserialize(deserializer)? {
        FlexibleDate::Timestamp(ts) => Ok(ts),
        FlexibleDate::DateString(s) => {
            chrono::NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S")
                .map(|dt| dt.and_utc().timestamp_millis())
                .map_err(|e| Error::custom(format!("Failed to parse date string: {}", e)))
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub code: String,
    pub message: String,
    pub data: Option<T>,
    /// 七云的业务异常响应不携带 success 字段，缺省按 false 处理，
    /// 以保证错误信息能正确透出而不是反序列化失败
    #[serde(default)]
    pub success: bool,
}

/// 读取七云响应并解析为 JSON。
///
/// 七云网关偶发返回非 JSON 内容（如 502/504 错误页、空响应体、服务端抖动时的空 xlsx），
/// 直接 `response.json()` 会产生无法定位的 `Decode` 错误（expected value, line 1 column 1）。
/// 这里先读取字节再解析，失败时带出 HTTP 状态码、Content-Type 与响应开头片段，
/// 并按 `classify_invalid_json` 区分可重试的服务端抖动与真正的协议错误。
async fn parse_json_response<T: serde::de::DeserializeOwned>(
    response: reqwest::Response,
) -> AppResult<T> {
    let status = response.status();
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let body = response.bytes().await?;
    serde_json::from_slice(&body)
        .map_err(|e| classify_invalid_json(&body, content_type.as_deref(), status, &e))
}

/// 判定无法解析为 JSON 的响应是否属于服务端抖动（可重试）：
/// - 响应体以 `PK\x03\x04` 开头：抖动时七云会现场生成并返回空 xlsx；
/// - 或响应 Content-Type 不含 json：七云正常响应为 application/json，
///   异常时 Content-Type 为空或返回 HTML 错误页。
///
/// 错误信息中携带 Content-Type 便于诊断。
fn classify_invalid_json(
    body: &[u8],
    content_type: Option<&str>,
    status: reqwest::StatusCode,
    parse_err: &serde_json::Error,
) -> AppError {
    let snippet: String = String::from_utf8_lossy(body).chars().take(200).collect();
    let message = format!(
        "Invalid JSON response from SevenCloud (HTTP {status}, Content-Type: {}): {parse_err}; body starts with: {snippet:?}",
        content_type.unwrap_or("<missing>")
    );
    let is_xlsx = body.starts_with(&XLSX_MAGIC);
    let is_json = content_type.is_some_and(|ct| ct.to_ascii_lowercase().contains("json"));
    if is_xlsx || !is_json {
        AppError::ExternalApiRetryable(message)
    } else {
        AppError::ExternalApiError(message)
    }
}

/// 判定业务失败响应是否属于七云服务端内部抖动（可退避重试，而非重登录）：
/// 抖动窗口内会返回 `code = B0001` 或 message 含「内部异常」的响应
/// （如 Feign 调用下游 `Connection refused executing GET http://szwl-server/...`）。
fn is_retryable_biz_error<T>(response: &ApiResponse<T>) -> bool {
    response.code == "B0001" || response.message.contains("内部异常")
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OrdersData {
    pub records: Vec<OrderRecord>,
    pub total: i64,
    pub size: i64,
    pub current: i64,
    pub pages: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OrderRecord {
    pub id: i64,
    #[serde(rename = "createDate")]
    pub create_date: i64,
    #[serde(rename = "memberCode")]
    pub member_code: Option<String>,
    pub price: Option<f64>,
    #[serde(rename = "productName")]
    pub product_name: String,
    #[serde(rename = "productNo")]
    pub product_no: Option<String>,
    pub status: i32,
    #[serde(rename = "payType")]
    pub pay_type: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CouponsData {
    pub records: Vec<CouponRecord>,
    pub total: i64,
    pub size: i64,
    pub current: i64,
    pub pages: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CouponRecord {
    pub id: i64,
    #[serde(rename = "adminId")]
    pub admin_id: Option<String>,
    #[serde(
        rename = "createDate",
        deserialize_with = "deserialize_flexible_date_required"
    )]
    pub create_date: i64,
    #[serde(rename = "userName")]
    pub user_name: Option<String>,
    #[serde(
        rename = "modifyDate",
        default,
        deserialize_with = "deserialize_flexible_date"
    )]
    pub modify_date: Option<i64>,
    pub code: i64,
    #[serde(rename = "isUse")]
    pub is_use: String,
    #[serde(
        rename = "useDate",
        default,
        deserialize_with = "deserialize_flexible_date"
    )]
    pub use_date: Option<i64>,
    #[serde(rename = "useBy")]
    pub use_by: Option<String>,
    #[serde(
        rename = "lastUseDate",
        default,
        deserialize_with = "deserialize_flexible_date"
    )]
    pub last_use_date: Option<i64>,
    pub discount: f64,
    #[serde(rename = "type")]
    pub coupon_type: Option<String>,
    #[serde(rename = "wxId")]
    pub wx_id: Option<String>,
}

pub struct SevenCloudAPI {
    client: Client,
    config: SevenCloudConfig,
    token: Option<String>,
    admin_id: Option<i64>,
    username: Option<String>,
    /// 各次重试前的退避间隔；尝试次数 = len + 1。测试中可替换为更短的间隔。
    retry_backoff: Vec<Duration>,
}

impl SevenCloudAPI {
    pub fn new(config: SevenCloudConfig) -> Self {
        Self {
            client: Client::new(),
            config,
            token: None,
            admin_id: None,
            username: None,
            retry_backoff: DEFAULT_RETRY_BACKOFF.to_vec(),
        }
    }

    /// 确保已登录并返回当前 token；未登录时先执行登录。
    /// 后台任务中长期持有 token 可能失效，每次请求前调用以获取最新 token。
    async fn ensure_token(&mut self) -> AppResult<String> {
        if self.token.is_none() {
            self.login().await?;
        }
        self.token.clone().ok_or_else(|| {
            AppError::ExternalApiError("Sevencloud login did not return a token".to_string())
        })
    }

    pub async fn login(&mut self) -> AppResult<()> {
        let url = format!("{}/SZWL-SERVER/tAdmin/loginSys", self.config.base_url);
        let password_hash = format!("{:x}", md5::compute(&self.config.password));

        let data = serde_json::json!({
            "username": self.config.username,
            "password": password_hash,
        });

        let response = self.client.post(&url).json(&data).send().await?;

        let result: ApiResponse<serde_json::Value> = parse_json_response(response).await?;

        if !result.success {
            return Err(AppError::ExternalApiError(format!(
                "Failed to login the sevencloud: {}",
                result.message
            )));
        }

        let data = result.data.ok_or_else(|| {
            AppError::ExternalApiError("Sevencloud response is empty".to_string())
        })?;

        self.admin_id = data["id"].as_i64();
        self.username = data["name"].as_str().map(|s| s.to_string());
        self.token = data["currentToken"].as_str().map(|s| s.to_string());

        log::info!(
            "Sevencloud API login successful, admin_id: {:?}",
            self.admin_id
        );

        Ok(())
    }

    /// 发送请求并解析响应，对七云服务端抖动做指数退避重试。
    ///
    /// - 可重试错误（非 JSON 抖动响应、`B0001`/`内部异常` 业务响应）按
    ///   `retry_backoff` 间隔重试，全部用尽后返回原错误；
    /// - 其余 `success: false` 视为鉴权失败，重登录一次后重试；
    /// - 请求构造闭包在每次尝试时重新调用，以携带最新 token。
    async fn send_with_retry<T, F>(
        &mut self,
        operation: &str,
        make_request: F,
    ) -> AppResult<ApiResponse<T>>
    where
        T: serde::de::DeserializeOwned,
        F: Fn(&Client, &str) -> reqwest::RequestBuilder,
    {
        let max_attempts = self.retry_backoff.len() + 1;
        let mut attempt = 0;
        let mut relogin_attempted = false;

        loop {
            attempt += 1;
            let token = self.ensure_token().await?;
            let response = make_request(&self.client, &token).send().await?;

            let result: ApiResponse<T> = match parse_json_response(response).await {
                Ok(r) => r,
                Err(e) => {
                    if e.is_retryable() && attempt < max_attempts {
                        let delay = self.retry_backoff[attempt - 1];
                        log::warn!(
                            "Failed to {operation}: retryable SevenCloud error (attempt {attempt}/{max_attempts}), retrying in {delay:?}: {e:?}"
                        );
                        tokio::time::sleep(delay).await;
                        continue;
                    }
                    return Err(e);
                }
            };

            if !result.success {
                if is_retryable_biz_error(&result) && attempt < max_attempts {
                    let delay = self.retry_backoff[attempt - 1];
                    log::warn!(
                        "Failed to {operation}: SevenCloud internal error (code: {}, message: {}), attempt {attempt}/{max_attempts}, retrying in {delay:?}",
                        result.code,
                        result.message
                    );
                    tokio::time::sleep(delay).await;
                    continue;
                }
                if !relogin_attempted {
                    log::warn!(
                        "Sevencloud token maybe expired when trying to {operation}, relogin and retry...: {}",
                        result.message
                    );
                    relogin_attempted = true;
                    self.login().await?;
                    continue;
                }
                return Err(AppError::ExternalApiError(format!(
                    "Failed to {operation}: {}",
                    result.message
                )));
            }

            return Ok(result);
        }
    }

    pub async fn get_orders(
        &mut self,
        start_date: &str,
        end_date: &str,
    ) -> AppResult<Vec<OrderRecord>> {
        let url = format!("{}/ORDER-SERVER/tOrder/pageOrder", self.config.base_url);
        let mut all_orders = Vec::new();
        let mut current_page = 1;

        // 确保已登录，避免在未登录时 panic
        self.ensure_token().await?;
        let admin_id = self.admin_id.ok_or_else(|| {
            AppError::ExternalApiError("Sevencloud login did not return an admin_id".to_string())
        })?;
        let username = self.username.clone().ok_or_else(|| {
            AppError::ExternalApiError("Sevencloud login did not return a username".to_string())
        })?;

        loop {
            let mut params = HashMap::new();
            params.insert("adminId", admin_id.to_string());
            params.insert("userName", username.clone());
            params.insert("adminType", "".to_string());
            params.insert("type", "".to_string());
            params.insert("payType", "".to_string());
            params.insert("productNo", "".to_string());
            params.insert("clientId", "".to_string());
            params.insert("dateType", "0".to_string());
            params.insert("startDate", start_date.to_string());
            params.insert("endDate", end_date.to_string());
            params.insert("current", current_page.to_string());
            params.insert("size", "1000".to_string());
            params.insert("status", "1".to_string());
            params.insert("companyType", "".to_string());
            params.insert("machineType", "".to_string());
            params.insert("ifForeign", "".to_string());
            params.insert("chartType", "day".to_string());

            // 对服务端抖动（空 xlsx、B0001 内部异常）做指数退避重试；
            // 其余失败判定为 token 失效时自动重登重试一次
            let result: ApiResponse<OrdersData> = self
                .send_with_retry("retrieve orders", |client, token| {
                    client
                        .get(&url)
                        .query(&params)
                        .header("Authorization", token)
                })
                .await?;
            let page_data = result
                .data
                .ok_or_else(|| AppError::ExternalApiError("Orders data is empty".to_string()))?;

            all_orders.extend(page_data.records);

            if current_page >= page_data.pages {
                break;
            }

            current_page += 1;
        }

        Ok(all_orders)
    }

    pub async fn get_discount_codes(
        &mut self,
        is_use: Option<bool>,
    ) -> AppResult<Vec<CouponRecord>> {
        let url = format!("{}/SZWL-SERVER/tPromoCode/list", self.config.base_url);
        let mut all_coupons = Vec::new();
        let mut current_page = 1;

        // 确保已登录，避免在未登录时 panic
        self.ensure_token().await?;
        let admin_id = self.admin_id.ok_or_else(|| {
            AppError::ExternalApiError("Sevencloud login did not return an admin_id".to_string())
        })?;

        loop {
            let mut data = serde_json::json!({
                "adminId": admin_id,
                "current": current_page,
                "size": 1000,
            });

            if let Some(is_use) = is_use {
                data["isUse"] =
                    serde_json::Value::String(if is_use { "1" } else { "0" }.to_string());
            }

            // 对服务端抖动（空 xlsx、B0001 内部异常）做指数退避重试；
            // 其余失败判定为 token 失效时自动重登重试一次
            let result: ApiResponse<CouponsData> = self
                .send_with_retry("retrieve discount codes", |client, token| {
                    client.post(&url).json(&data).header("Authorization", token)
                })
                .await?;
            let page_data = result.data.ok_or_else(|| {
                AppError::ExternalApiError("Discount codes data is empty".to_string())
            })?;

            all_coupons.extend(page_data.records);
            if current_page >= page_data.pages {
                break;
            }
            current_page += 1;
        }

        Ok(all_coupons)
    }

    /// 生成优惠码
    ///
    /// # 参数
    /// * `code` - 优惠码
    /// * `discount` - 折扣金额（固定金额时单位为美元，百分比时单位为折数，如 7.5）
    /// * `discount_type` - 折扣类型：0=百分比，1=固定金额
    /// * `expire_months` - 过期月份
    ///
    /// # 返回值
    /// 返回一个布尔值，表示优惠码是否生成成功。
    pub async fn generate_discount_code(
        &mut self,
        code: &str,
        discount: f64,
        discount_type: u32,
        expire_months: u32,
    ) -> AppResult<bool> {
        if code.len() != 6 || !code.chars().all(|c| c.is_ascii_digit()) {
            return Err(AppError::ValidationError(
                "Invalid discount code format".to_string(),
            ));
        }

        if discount <= 0.0 {
            return Err(AppError::ValidationError(
                "Discount amount must be greater than 0".to_string(),
            ));
        }

        if expire_months == 0 || expire_months > 3 {
            return Err(AppError::ValidationError(
                "Expiration period must be between 1-3 months".to_string(),
            ));
        }

        let url = format!("{}/SZWL-SERVER/tPromoCode/add", self.config.base_url);

        // 确保已登录，避免在未登录时 panic
        self.ensure_token().await?;
        let admin_id = self.admin_id.ok_or_else(|| {
            AppError::ExternalApiError("Sevencloud login did not return an admin_id".to_string())
        })?;

        let mut params = HashMap::new();
        params.insert("addMode", "2".to_string());
        params.insert("codeNum", code.to_string());
        params.insert("number", "1".to_string());
        params.insert("month", expire_months.to_string());
        params.insert("type", discount_type.to_string());
        params.insert("discount", discount.to_string());
        params.insert("frpCode", "WEIXIN_NATIVE".to_string());
        params.insert("adminId", admin_id.to_string());

        // 对服务端抖动（空 xlsx、B0001 内部异常）做指数退避重试；
        // 其余失败判定为 token 失效时自动重登重试一次
        let _result: ApiResponse<String> = self
            .send_with_retry("generate discount code", |client, token| {
                client
                    .get(&url)
                    .query(&params)
                    .header("Authorization", token)
            })
            .await?;

        log::info!(
            "Successfully generated discount code: {code}, discount_type: {discount_type}, discount: {discount}, Expiration: {expire_months} months"
        );

        Ok(true)
    }

    /// 删除优惠码
    ///
    /// # 参数
    /// * `ids` - 优惠码的 external_id 列表
    ///
    /// # 返回值
    /// 返回一个布尔值，表示是否删除成功。
    pub async fn delete_discount_codes(&mut self, ids: Vec<i64>) -> AppResult<bool> {
        if ids.is_empty() {
            return Ok(true);
        }

        let url = format!("{}/SZWL-SERVER/tPromoCode/deletes", self.config.base_url);

        let body = serde_json::json!(ids);

        // 对服务端抖动（空 xlsx、B0001 内部异常）做指数退避重试；
        // 其余失败判定为 token 失效时自动重登重试一次
        let _result: ApiResponse<String> = self
            .send_with_retry("delete discount codes", |client, token| {
                client.post(&url).json(&body).header("Authorization", token)
            })
            .await?;

        log::info!("Successfully deleted discount codes: {:?}", ids);

        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    fn parse_err() -> serde_json::Error {
        serde_json::from_str::<serde_json::Value>("not json").unwrap_err()
    }

    // ---- 错误分类 ----

    #[test]
    fn xlsx_body_is_classified_as_retryable() {
        let body = b"PK\x03\x04\x14\x00\x06\x00...[Content_Types].xml";
        let err = classify_invalid_json(body, None, reqwest::StatusCode::OK, &parse_err());
        assert!(
            err.is_retryable(),
            "empty xlsx body should be retryable: {err:?}"
        );
    }

    #[test]
    fn missing_or_non_json_content_type_is_retryable() {
        let body = b"unexpected";
        for ct in [None, Some(""), Some("text/html")] {
            let err = classify_invalid_json(body, ct, reqwest::StatusCode::OK, &parse_err());
            assert!(
                err.is_retryable(),
                "content-type {ct:?} should be retryable"
            );
        }
    }

    #[test]
    fn invalid_json_with_json_content_type_is_not_retryable() {
        let body = b"{broken";
        let err = classify_invalid_json(
            body,
            Some("application/json"),
            reqwest::StatusCode::OK,
            &parse_err(),
        );
        assert!(matches!(err, AppError::ExternalApiError(_)));
    }

    #[test]
    fn classification_error_message_carries_content_type() {
        let err = classify_invalid_json(
            b"PK\x03\x04xxx",
            None,
            reqwest::StatusCode::OK,
            &parse_err(),
        );
        assert!(err.to_string().contains("Content-Type: <missing>"));
        let err = classify_invalid_json(
            b"PK\x03\x04xxx",
            Some("application/octet-stream"),
            reqwest::StatusCode::OK,
            &parse_err(),
        );
        assert!(
            err.to_string()
                .contains("Content-Type: application/octet-stream")
        );
    }

    // ---- 业务错误分类 ----

    fn biz_error(code: &str, message: &str) -> ApiResponse<serde_json::Value> {
        ApiResponse {
            code: code.to_string(),
            message: message.to_string(),
            data: None,
            success: false,
        }
    }

    #[test]
    fn b0001_internal_error_is_retryable() {
        assert!(is_retryable_biz_error(&biz_error(
            "B0001",
            "内部异常[Connection refused executing GET http://szwl-server/tAdmin/getAdmin?id=3888]"
        )));
        assert!(is_retryable_biz_error(&biz_error("B0001", "anything")));
        assert!(is_retryable_biz_error(&biz_error("X9999", "内部异常")));
    }

    #[test]
    fn auth_like_biz_error_is_not_retryable() {
        assert!(!is_retryable_biz_error(&biz_error(
            "A0001",
            "token expired"
        )));
        assert!(!is_retryable_biz_error(&biz_error("", "用户名或密码错误")));
    }

    // ---- 重试链路（mock server）----

    #[derive(Clone, Copy)]
    enum FailureMode {
        /// 200 OK + 空 xlsx 字节、无 Content-Type
        Xlsx,
        /// 200 OK + B0001 内部异常 JSON
        InternalError,
    }

    struct MockServer {
        base_url: String,
        login_count: Arc<AtomicUsize>,
        order_count: Arc<AtomicUsize>,
    }

    /// 启动一个最小 mock server：登录接口返回正常 JSON；
    /// 订单接口前 `order_failures` 次按 `mode` 返回抖动响应，之后返回正常 JSON。
    async fn spawn_mock(order_failures: usize, mode: FailureMode) -> MockServer {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let login_count = Arc::new(AtomicUsize::new(0));
        let order_count = Arc::new(AtomicUsize::new(0));
        let (lc, oc) = (login_count.clone(), order_count.clone());

        tokio::spawn(async move {
            loop {
                let (mut socket, _) = match listener.accept().await {
                    Ok(v) => v,
                    Err(_) => break,
                };
                let (lc, oc) = (lc.clone(), oc.clone());
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 8192];
                    let n = socket.read(&mut buf).await.unwrap_or(0);
                    let req = String::from_utf8_lossy(&buf[..n]);

                    let (content_type, body): (Option<&str>, Vec<u8>) = if req
                        .starts_with("POST /SZWL-SERVER/tAdmin/loginSys")
                    {
                        lc.fetch_add(1, Ordering::SeqCst);
                        let body = serde_json::json!({
                            "code": "000000",
                            "message": "success",
                            "success": true,
                            "data": {"id": 3888, "name": "tester", "currentToken": "token-abc"}
                        });
                        (Some("application/json"), body.to_string().into_bytes())
                    } else {
                        let nth = oc.fetch_add(1, Ordering::SeqCst) + 1;
                        if nth <= order_failures {
                            match mode {
                                FailureMode::Xlsx => (
                                    None,
                                    b"PK\x03\x04\x14\x00\x06\x00...[Content_Types].xml".to_vec(),
                                ),
                                FailureMode::InternalError => {
                                    let body = serde_json::json!({
                                        "code": "B0001",
                                        "message": "内部异常[Connection refused executing GET http://szwl-server/tAdmin/getAdmin?id=3888]",
                                        "success": false
                                    });
                                    (Some("application/json"), body.to_string().into_bytes())
                                }
                            }
                        } else {
                            let body = serde_json::json!({
                                "code": "000000",
                                "message": "success",
                                "success": true,
                                "data": {
                                    "records": [{
                                        "id": 1,
                                        "createDate": 1700000000000i64,
                                        "memberCode": "M1",
                                        "price": 9.9,
                                        "productName": "coffee",
                                        "productNo": "P1",
                                        "status": 1,
                                        "payType": 1
                                    }],
                                    "total": 1,
                                    "size": 1000,
                                    "current": 1,
                                    "pages": 1
                                }
                            });
                            (Some("application/json"), body.to_string().into_bytes())
                        }
                    };

                    let mut head = format!(
                        "HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n",
                        body.len()
                    );
                    if let Some(ct) = content_type {
                        head.push_str(&format!("content-type: {ct}\r\n"));
                    }
                    head.push_str("\r\n");
                    let _ = socket.write_all(head.as_bytes()).await;
                    let _ = socket.write_all(&body).await;
                });
            }
        });

        MockServer {
            base_url: format!("http://{addr}"),
            login_count,
            order_count,
        }
    }

    fn test_api(base_url: String) -> SevenCloudAPI {
        let mut api = SevenCloudAPI::new(SevenCloudConfig {
            username: "u".to_string(),
            password: "p".to_string(),
            base_url,
        });
        // 测试中使用毫秒级退避，保持用例快速
        api.retry_backoff = vec![Duration::from_millis(10); 4];
        api
    }

    #[tokio::test]
    async fn get_orders_recovers_from_transient_xlsx_response() {
        let server = spawn_mock(2, FailureMode::Xlsx).await;
        let mut api = test_api(server.base_url.clone());

        let orders = api.get_orders("2025-01-01", "2025-01-02").await.unwrap();

        assert_eq!(orders.len(), 1);
        assert_eq!(orders[0].id, 1);
        assert_eq!(server.order_count.load(Ordering::SeqCst), 3);
        assert_eq!(server.login_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn get_orders_fails_with_retryable_error_after_exhausting_retries() {
        let server = spawn_mock(usize::MAX, FailureMode::Xlsx).await;
        let mut api = test_api(server.base_url.clone());

        let err = api
            .get_orders("2025-01-01", "2025-01-02")
            .await
            .unwrap_err();

        match err {
            AppError::ExternalApiRetryable(msg) => {
                assert!(msg.contains("Content-Type: <missing>"), "{msg}");
                assert!(msg.contains("PK\\u{3}\\u{4}"), "{msg}");
            }
            other => panic!("expected ExternalApiRetryable, got: {other:?}"),
        }
        // 1 + 4 次重试，共 5 次尝试
        assert_eq!(server.order_count.load(Ordering::SeqCst), 5);
        assert_eq!(server.login_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn get_orders_retries_internal_error_without_relogin() {
        let server = spawn_mock(2, FailureMode::InternalError).await;
        let mut api = test_api(server.base_url.clone());

        let orders = api.get_orders("2025-01-01", "2025-01-02").await.unwrap();

        assert_eq!(orders.len(), 1);
        assert_eq!(server.order_count.load(Ordering::SeqCst), 3);
        // B0001 内部异常走退避重试，不触发重登录
        assert_eq!(server.login_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn get_orders_relogins_once_on_auth_failure() {
        // 服务端始终返回非抖动类业务失败，应重登录一次后报错，且不做退避重试
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let login_count = Arc::new(AtomicUsize::new(0));
        let lc = login_count.clone();
        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                let lc = lc.clone();
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 8192];
                    let n = socket.read(&mut buf).await.unwrap_or(0);
                    let req = String::from_utf8_lossy(&buf[..n]);
                    let body = if req.starts_with("POST /SZWL-SERVER/tAdmin/loginSys") {
                        lc.fetch_add(1, Ordering::SeqCst);
                        serde_json::json!({
                            "code": "000000",
                            "message": "success",
                            "success": true,
                            "data": {"id": 3888, "name": "tester", "currentToken": "token-abc"}
                        })
                    } else {
                        serde_json::json!({
                            "code": "A0400",
                            "message": "token expired",
                            "success": false
                        })
                    };
                    let body = body.to_string();
                    let head = format!(
                        "HTTP/1.1 200 OK\r\ncontent-length: {}\r\ncontent-type: application/json\r\nconnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = socket.write_all(head.as_bytes()).await;
                    let _ = socket.write_all(body.as_bytes()).await;
                });
            }
        });

        let mut api = test_api(format!("http://{addr}"));
        let err = api
            .get_orders("2025-01-01", "2025-01-02")
            .await
            .unwrap_err();

        match err {
            AppError::ExternalApiError(msg) => {
                assert!(
                    msg.contains("Failed to retrieve orders: token expired"),
                    "{msg}"
                )
            }
            other => panic!("expected ExternalApiError, got: {other:?}"),
        }
        // 首次登录 + 一次重登录
        assert_eq!(login_count.load(Ordering::SeqCst), 2);
    }
}
