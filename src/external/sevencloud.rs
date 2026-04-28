use crate::config::SevenCloudConfig;
use crate::error::{AppError, AppResult};
use reqwest::Client;
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;

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
    pub success: bool,
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
}

impl SevenCloudAPI {
    pub fn new(config: SevenCloudConfig) -> Self {
        Self {
            client: Client::new(),
            config,
            token: None,
            admin_id: None,
            username: None,
        }
    }

    pub async fn login(&mut self) -> AppResult<()> {
        let url = format!("{}/SZWL-SERVER/tAdmin/loginSys", self.config.base_url);
        let password_hash = format!("{:x}", md5::compute(&self.config.password));

        let data = serde_json::json!({
            "username": self.config.username,
            "password": password_hash,
        });

        let response = self.client.post(&url).json(&data).send().await?;

        let result: ApiResponse<serde_json::Value> = response.json().await?;

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
            self.admin_id.unwrap()
        );

        Ok(())
    }

    pub async fn get_orders(
        &mut self,
        start_date: &str,
        end_date: &str,
    ) -> AppResult<Vec<OrderRecord>> {
        let url = format!("{}/ORDER-SERVER/tOrder/pageOrder", self.config.base_url);
        let mut all_orders = Vec::new();
        let mut current_page = 1;

        loop {
            let mut params = HashMap::new();
            params.insert("adminId", self.admin_id.unwrap().to_string());
            params.insert("userName", self.username.as_ref().unwrap().clone());
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

            // 最多尝试 2 次 (第一次失败且判定为 token 失效时自动重登重试)
            let mut attempt = 0;
            let page_data = loop {
                attempt += 1;
                let response = self
                    .client
                    .get(&url)
                    .query(&params)
                    .header("Authorization", self.token.as_ref().unwrap())
                    .send()
                    .await?;

                let result: ApiResponse<OrdersData> = response.json().await?;

                if !result.success {
                    if attempt == 1 {
                        log::warn!(
                            "Sevencloud token maybe expired when fetching orders, relogin and retry...: {}",
                            result.message
                        );
                        self.login().await?; // 重新登录并重试
                        continue;
                    }
                    return Err(AppError::ExternalApiError(format!(
                        "Failed to retrieve orders: {}",
                        result.message
                    )));
                }

                let data = result.data.ok_or_else(|| {
                    AppError::ExternalApiError("Orders data is empty".to_string())
                })?;
                break data;
            };

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

        loop {
            let mut data = serde_json::json!({
                "adminId": self.admin_id.unwrap(),
                "current": current_page,
                "size": 1000,
            });

            if let Some(is_use) = is_use {
                data["isUse"] =
                    serde_json::Value::String(if is_use { "1" } else { "0" }.to_string());
            }

            let mut attempt = 0;
            let page_data = loop {
                attempt += 1;
                let response = self
                    .client
                    .post(&url)
                    .json(&data)
                    .header("Authorization", self.token.as_ref().unwrap())
                    .send()
                    .await?;

                let result: ApiResponse<CouponsData> = response.json().await?;
                if !result.success {
                    if attempt == 1 {
                        log::warn!(
                            "Sevencloud token maybe expired when fetching discount codes, relogin and retry...: {}",
                            result.message
                        );
                        self.login().await?;
                        continue;
                    }
                    return Err(AppError::ExternalApiError(format!(
                        "Failed to retrieve discount codes: {}",
                        result.message
                    )));
                }
                let data = result.data.ok_or_else(|| {
                    AppError::ExternalApiError("Discount codes data is empty".to_string())
                })?;
                break data;
            };

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

        let mut params = HashMap::new();
        params.insert("addMode", "2".to_string());
        params.insert("codeNum", code.to_string());
        params.insert("number", "1".to_string());
        params.insert("month", expire_months.to_string());
        params.insert("type", discount_type.to_string());
        params.insert("discount", discount.to_string());
        params.insert("frpCode", "WEIXIN_NATIVE".to_string());
        params.insert("adminId", self.admin_id.unwrap().to_string());

        let mut attempt = 0;
        let _result = loop {
            attempt += 1;
            let response = self
                .client
                .get(&url)
                .query(&params)
                .header("Authorization", self.token.as_ref().unwrap())
                .send()
                .await?;
            let result: ApiResponse<String> = response.json().await?;
            if !result.success {
                if attempt == 1 {
                    log::warn!(
                        "Sevencloud token maybe expired when generating discount code, relogin and retry...: {}",
                        result.message
                    );
                    self.login().await?;
                    continue;
                }
                return Err(AppError::ExternalApiError(format!(
                    "Failed to generate discount code: {}",
                    result.message
                )));
            }
            break result;
        };

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

        let mut attempt = 0;
        let _result = loop {
            attempt += 1;
            let response = self
                .client
                .post(&url)
                .json(&body)
                .header("Authorization", self.token.as_ref().unwrap())
                .send()
                .await?;
            let result: ApiResponse<String> = response.json().await?;
            if !result.success {
                if attempt == 1 {
                    log::warn!(
                        "Sevencloud token maybe expired when deleting discount codes, relogin and retry...: {}",
                        result.message
                    );
                    self.login().await?;
                    continue;
                }
                return Err(AppError::ExternalApiError(format!(
                    "Failed to delete discount codes: {}",
                    result.message
                )));
            }
            break result;
        };

        log::info!("Successfully deleted discount codes: {:?}", ids);

        Ok(true)
    }
}
