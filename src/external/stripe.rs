use crate::config::StripeConfig;
use crate::error::{AppError, AppResult};
use std::collections::HashMap;
use std::str::FromStr;
use stripe::{
    CheckoutSession, CheckoutSessionMode, Client, CreateCheckoutSession,
    CreateCheckoutSessionLineItems, CreateCheckoutSessionLineItemsPriceData,
    CreateCheckoutSessionLineItemsPriceDataProductData, CreateCheckoutSessionPaymentIntentData,
    CreatePaymentIntent, CreatePaymentIntentAutomaticPaymentMethods, Currency, Event, Expandable,
    PaymentIntent, PaymentIntentId, Price as StripePrice, PriceId,
};

/// Stripe服务，用于处理支付意图和webhook验证
///
/// 这个服务专为任意金额充值设计，而不是预制商品类型的支付。
/// 支持多种货币，自动启用多种支付方式，并包含完整的金额验证。
///
/// # 示例
///
/// ```ignore
/// use kkss_backend::external::stripe::{StripeService, StripeConfig};
///
/// let config = StripeConfig {
///     secret_key: "sk_test_...".to_string(),
///     webhook_secret: "whsec_...".to_string(),
/// };
/// let stripe_service = StripeService::new(config);
///
/// // 创建$10.00的充值支付意图
/// let amount_cents = StripeService::dollars_to_cents(10.00);
/// let payment_intent = stripe_service.create_payment_intent(
///     amount_cents,
///     123, // user_id
///     Some("usd".to_string()),
///     Some("用户充值".to_string())
/// ).await?;
/// ```
#[derive(Clone)]
pub struct StripeService {
    client: Client,
    config: StripeConfig,
}

#[derive(Clone, Debug)]
pub struct CheckoutInit {
    pub url: String,
    pub payment_intent_id: Option<String>,
    pub client_secret: Option<String>,
}

impl StripeService {
    pub fn new(config: StripeConfig) -> Self {
        let client = Client::new(&config.secret_key);
        Self { client, config }
    }

    /// 创建 Stripe Checkout Session（基于 price_id 的单个商品）并返回 URL
    pub async fn create_checkout_session_with_price(
        &self,
        user_id: i64,
        category: &str,
        price_id: &str,
        quantity: u64,
        description: Option<String>,
        extra_metadata: Option<HashMap<String, String>>,
    ) -> AppResult<CheckoutInit> {
        let success_url =
            self.config.checkout_success_url.clone().ok_or_else(|| {
                AppError::InternalError("Missing STRIPE_CHECKOUT_SUCCESS_URL".into())
            })?;
        let cancel_url =
            self.config.checkout_cancel_url.clone().ok_or_else(|| {
                AppError::InternalError("Missing STRIPE_CHECKOUT_CANCEL_URL".into())
            })?;

        let mut create = CreateCheckoutSession::new();
        let success_ref = success_url;
        let cancel_ref = cancel_url;
        create.success_url = Some(&success_ref);
        create.cancel_url = Some(&cancel_ref);
        create.mode = Some(CheckoutSessionMode::Payment);
        create.line_items = Some(vec![CreateCheckoutSessionLineItems {
            price: Some(price_id.to_string()),
            quantity: Some(quantity),
            ..Default::default()
        }]);
        // 元数据记录 user/category
        let mut meta = std::collections::HashMap::new();
        meta.insert("user_id".to_string(), user_id.to_string());
        meta.insert("category".to_string(), category.to_string());
        if let Some(extra) = extra_metadata {
            for (k, v) in extra.into_iter() {
                meta.insert(k, v);
            }
        }
        create.metadata = Some(meta.clone());
        let client_ref = user_id.to_string();
        create.client_reference_id = Some(&client_ref);
        create.payment_intent_data = Some(CreateCheckoutSessionPaymentIntentData {
            description,
            metadata: Some(meta),
            ..Default::default()
        });

        let session = CheckoutSession::create(&self.client, create)
            .await
            .map_err(|e| {
                AppError::ExternalApiError(format!("Failed to create checkout session: {e}"))
            })?;
        let url = session
            .url
            .ok_or_else(|| AppError::ExternalApiError("Missing checkout url".into()))?;
        // 提取 PaymentIntent 信息
        let (pi_id_opt, client_secret) = match session.payment_intent {
            Some(Expandable::Id(ref id)) => {
                // 取回 PaymentIntent 以获取 client_secret
                let pi = PaymentIntent::retrieve(&self.client, id, &[])
                    .await
                    .map_err(|e| {
                        AppError::ExternalApiError(format!(
                            "Failed to retrieve PaymentIntent after session create: {e}"
                        ))
                    })?;
                (Some(id.to_string()), pi.client_secret)
            }
            Some(Expandable::Object(ref obj)) => {
                (Some(obj.id.to_string()), obj.client_secret.clone())
            }
            None => (None, None),
        };
        Ok(CheckoutInit {
            url,
            payment_intent_id: pi_id_opt,
            client_secret,
        })
    }

    /// 创建 Stripe Checkout Session（基于自定义金额，底层仍走 PaymentIntent）并返回 URL
    pub async fn create_checkout_session_for_amount(
        &self,
        amount: i64,
        currency: Option<String>,
        user_id: i64,
        category: &str,
        description: Option<String>,
        extra_metadata: Option<HashMap<String, String>>,
    ) -> AppResult<CheckoutInit> {
        // 金额校验
        if amount < 50 {
            return Err(AppError::ValidationError("Minimum amount is $0.50".into()));
        }
        let success_url =
            self.config.checkout_success_url.clone().ok_or_else(|| {
                AppError::InternalError("Missing STRIPE_CHECKOUT_SUCCESS_URL".into())
            })?;
        let cancel_url =
            self.config.checkout_cancel_url.clone().ok_or_else(|| {
                AppError::InternalError("Missing STRIPE_CHECKOUT_CANCEL_URL".into())
            })?;

        // 解析货币
        let currency = currency.unwrap_or_else(|| "usd".to_string());
        let currency = match currency.to_lowercase().as_str() {
            "usd" => Currency::USD,
            "eur" => Currency::EUR,
            "gbp" => Currency::GBP,
            "jpy" => Currency::JPY,
            "cad" => Currency::CAD,
            "aud" => Currency::AUD,
            _ => Currency::USD,
        };

        let mut meta = HashMap::new();
        meta.insert("user_id".to_string(), user_id.to_string());
        meta.insert("category".to_string(), category.to_string());
        if let Some(extra) = extra_metadata {
            for (k, v) in extra.into_iter() {
                meta.insert(k, v);
            }
        }

        // 通过 line_items.price_data 传金额/货币，并在 payment_intent_data 放描述与元数据
        let mut create = CreateCheckoutSession::new();
        let success_ref = success_url;
        let cancel_ref = cancel_url;
        create.success_url = Some(&success_ref);
        create.cancel_url = Some(&cancel_ref);
        create.mode = Some(CheckoutSessionMode::Payment);
        create.line_items = Some(vec![CreateCheckoutSessionLineItems {
            price_data: Some(CreateCheckoutSessionLineItemsPriceData {
                currency,
                unit_amount: Some(amount),
                product_data: Some(CreateCheckoutSessionLineItemsPriceDataProductData {
                    name: description.clone().unwrap_or_else(|| "Payment".to_string()),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            quantity: Some(1),
            ..Default::default()
        }]);
        // 将元数据同时放在 Session 与 PaymentIntent 中，便于不同 webhook 对象读取
        create.metadata = Some(meta.clone());
        let client_ref = user_id.to_string();
        create.client_reference_id = Some(&client_ref);
        create.payment_intent_data = Some(CreateCheckoutSessionPaymentIntentData {
            description,
            metadata: Some(meta),
            ..Default::default()
        });
        let session = CheckoutSession::create(&self.client, create)
            .await
            .map_err(|e| {
                AppError::ExternalApiError(format!("Failed to create checkout session: {e}"))
            })?;
        let url = session
            .url
            .ok_or_else(|| AppError::ExternalApiError("Missing checkout url".into()))?;
        let (pi_id_opt, client_secret) = match session.payment_intent {
            Some(Expandable::Id(ref id)) => {
                let pi = PaymentIntent::retrieve(&self.client, id, &[])
                    .await
                    .map_err(|e| {
                        AppError::ExternalApiError(format!(
                            "Failed to retrieve PaymentIntent after session create: {e}"
                        ))
                    })?;
                (Some(id.to_string()), pi.client_secret)
            }
            Some(Expandable::Object(ref obj)) => {
                (Some(obj.id.to_string()), obj.client_secret.clone())
            }
            None => (None, None),
        };
        Ok(CheckoutInit {
            url,
            payment_intent_id: pi_id_opt,
            client_secret,
        })
    }

    /// 创建用于任意金额充值的支付意图
    ///
    /// # 参数
    ///
    /// * `amount` - 充值金额，以最小货币单位计算（如美分）
    /// * `user_id` - 用户ID，会存储在metadata中
    /// * `currency` - 货币代码（如"usd", "eur"），默认为"usd"
    /// * `description` - 支付描述，如果为None会自动生成
    ///
    /// # 返回
    ///
    /// 返回包含client_secret的PaymentIntent，客户端可用此完成支付
    ///
    /// # 错误
    ///
    /// * 如果金额小于最小值（$0.50）会返回ValidationError
    /// * 如果Stripe API调用失败会返回ExternalApiError
    pub async fn create_payment_intent(
        &self,
        amount: i64,
        user_id: i64,
        currency: Option<String>,
        description: Option<String>,
    ) -> AppResult<PaymentIntent> {
        self.create_payment_intent_with_category(
            amount,
            user_id,
            "recharge",
            currency,
            description,
            None,
        )
        .await
    }

    /// 读取某个 Price 的单位金额（单位：最小货币单位，如美分）
    pub async fn get_price_unit_amount(&self, price_id: &str) -> AppResult<i64> {
        let pid = PriceId::from_str(price_id)
            .map_err(|e| AppError::ValidationError(format!("Invalid price id: {e}")))?;
        let price = StripePrice::retrieve(&self.client, &pid, &[])
            .await
            .map_err(|e| {
                AppError::ExternalApiError(format!("Failed to retrieve price {price_id}: {e}"))
            })?;
        let amt = price.unit_amount.ok_or_else(|| {
            AppError::ValidationError(format!("Price {price_id} has no unit_amount configured"))
        })?;
        Ok(amt)
    }

    /// 返回月卡产品与价格ID（product, one_time_price, subscription_price）
    pub fn monthly_card_ids(&self) -> (Option<String>, Option<String>, Option<String>) {
        (
            self.config.monthly_card_product_id.clone(),
            self.config.monthly_card_one_time_price_id.clone(),
            self.config.monthly_card_subscription_price_id.clone(),
        )
    }

    /// 创建带有业务类别与自定义 metadata 的支付意图
    pub async fn create_payment_intent_with_category(
        &self,
        amount: i64,
        user_id: i64,
        category: &str,
        currency: Option<String>,
        description: Option<String>,
        extra_metadata: Option<HashMap<String, String>>,
    ) -> AppResult<PaymentIntent> {
        // 验证最小金额 (50美分 = $0.50)
        if amount < 50 {
            return Err(AppError::ValidationError(
                "Minimum amount is $0.50".to_string(),
            ));
        }

        // 解析货币类型
        let currency = currency.unwrap_or_else(|| "usd".to_string());
        let currency = match currency.to_lowercase().as_str() {
            "usd" => Currency::USD,
            "eur" => Currency::EUR,
            "gbp" => Currency::GBP,
            "jpy" => Currency::JPY,
            "cad" => Currency::CAD,
            "aud" => Currency::AUD,
            _ => Currency::USD, // 默认使用USD
        };

        // 创建metadata
        let mut metadata = HashMap::new();
        metadata.insert("user_id".to_string(), user_id.to_string());
        metadata.insert("category".to_string(), category.to_string());
        if let Some(extra) = extra_metadata {
            for (k, v) in extra.into_iter() {
                metadata.insert(k, v);
            }
        }

        // 设置描述
        let description = description
            .unwrap_or_else(|| format!("Recharge ${:.2} to account", amount as f64 / 100.0));

        // 创建支付意图请求
        let mut create_payment_intent = CreatePaymentIntent::new(amount, currency);
        create_payment_intent.description = Some(&description);
        create_payment_intent.metadata = Some(metadata);

        // 启用自动支付方式
        create_payment_intent.automatic_payment_methods =
            Some(CreatePaymentIntentAutomaticPaymentMethods {
                enabled: true,
                allow_redirects: None,
            });

        // 发送请求
        let payment_intent = PaymentIntent::create(&self.client, create_payment_intent)
            .await
            .map_err(|e| {
                AppError::ExternalApiError(format!("Failed to create payment intent: {e}"))
            })?;

        Ok(payment_intent)
    }

    /// 检索已存在的支付意图
    ///
    /// # 参数
    ///
    /// * `payment_intent_id` - Stripe支付意图ID
    ///
    /// # 返回
    ///
    /// 返回PaymentIntent对象，包含当前状态和详细信息
    pub async fn retrieve_payment_intent(
        &self,
        payment_intent_id: &str,
    ) -> AppResult<PaymentIntent> {
        let payment_intent_id = PaymentIntentId::from_str(payment_intent_id)
            .map_err(|e| AppError::ValidationError(format!("Invalid payment intent ID: {e}")))?;

        let payment_intent = PaymentIntent::retrieve(&self.client, &payment_intent_id, &[])
            .await
            .map_err(|e| {
                AppError::ExternalApiError(format!("Failed to retrieve payment intent: {e}"))
            })?;

        Ok(payment_intent)
    }

    /// 验证Stripe Webhook签名
    ///
    /// # 参数
    ///
    /// * `payload` - webhook请求体
    /// * `signature` - Stripe-Signature头
    /// * `timestamp` - 时间戳（当前未使用）
    ///
    /// # 返回
    ///
    /// 成功时返回Ok(())，失败时返回相应错误
    pub fn verify_webhook_signature(
        &self,
        payload: &str,
        signature: &str,
        _timestamp: i64,
    ) -> AppResult<Event> {
        // 验证webhook签名头是否存在
        if signature.is_empty() {
            return Err(AppError::AuthError("Invalid webhook signature".to_string()));
        }

        // 使用async-stripe的webhook验证
        let event =
            stripe::Webhook::construct_event(payload, signature, &self.config.webhook_secret)
                .map_err(|e| {
                    AppError::AuthError(format!("Webhook signature verification failed: {e}"))
                })?;

        Ok(event)
    }

    /// 将美元金额转换为美分
    ///
    /// # 示例
    ///
    /// ```
    /// use kkss_backend::external::stripe::StripeService;
    ///
    /// assert_eq!(StripeService::dollars_to_cents(10.99), 1099);
    /// assert_eq!(StripeService::dollars_to_cents(0.50), 50);
    /// ```
    pub fn dollars_to_cents(dollars: f64) -> i64 {
        (dollars * 100.0).round() as i64
    }

    /// 将美分转换为美元金额
    ///
    /// # 示例
    ///
    /// ```
    /// use kkss_backend::external::stripe::StripeService;
    ///
    /// assert_eq!(StripeService::cents_to_dollars(1099), 10.99);
    /// assert_eq!(StripeService::cents_to_dollars(50), 0.50);
    /// ```
    pub fn cents_to_dollars(cents: i64) -> f64 {
        cents as f64 / 100.0
    }

    /// 验证金额是否符合Stripe的要求
    ///
    /// 根据不同货币检查最小和最大金额限制。
    ///
    /// # 参数
    ///
    /// * `amount` - 金额，以最小货币单位计算
    /// * `currency` - 货币代码（如"usd", "eur", "jpy"）
    ///
    /// # 错误
    ///
    /// * 如果金额小于最小值会返回ValidationError
    /// * 如果金额超过最大值会返回ValidationError
    pub fn validate_amount(amount: i64, currency: &str) -> AppResult<()> {
        let min_amount = match currency.to_lowercase().as_str() {
            "usd" | "eur" | "cad" | "aud" | "gbp" => 50, // $0.50
            "jpy" => 50,                                 // ¥50 (日元没有小数)
            _ => 50,                                     // 默认最小值
        };

        if amount < min_amount {
            return Err(AppError::ValidationError(format!(
                "Minimum recharge amount is {} {}",
                if currency == "jpy" {
                    format!("{min_amount}")
                } else {
                    format!("{:.2}", min_amount as f64 / 100.0)
                },
                currency.to_uppercase()
            )));
        }

        // Stripe支持的最大金额是99999999 (约$999,999.99)
        if amount > 99999999 {
            return Err(AppError::ValidationError(
                "Maximum recharge amount is $999,999.99".to_string(),
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dollars_to_cents_conversion() {
        assert_eq!(StripeService::dollars_to_cents(1.00), 100);
        assert_eq!(StripeService::dollars_to_cents(0.50), 50);
        assert_eq!(StripeService::dollars_to_cents(10.99), 1099);
        assert_eq!(StripeService::dollars_to_cents(0.01), 1);
    }

    #[test]
    fn test_cents_to_dollars_conversion() {
        assert_eq!(StripeService::cents_to_dollars(100), 1.00);
        assert_eq!(StripeService::cents_to_dollars(50), 0.50);
        assert_eq!(StripeService::cents_to_dollars(1099), 10.99);
        assert_eq!(StripeService::cents_to_dollars(1), 0.01);
    }

    #[test]
    fn test_amount_validation() {
        // 测试有效金额
        assert!(StripeService::validate_amount(100, "usd").is_ok()); // $1.00
        assert!(StripeService::validate_amount(50, "usd").is_ok()); // $0.50 (最小值)

        // 测试无效金额 (小于最小值)
        assert!(StripeService::validate_amount(49, "usd").is_err());
        assert!(StripeService::validate_amount(0, "usd").is_err());

        // 测试超大金额
        assert!(StripeService::validate_amount(100000000, "usd").is_err());

        // 测试日元 (无小数位)
        assert!(StripeService::validate_amount(50, "jpy").is_ok());
        assert!(StripeService::validate_amount(49, "jpy").is_err());
    }
}
