use crate::config::EmailConfig;
use crate::error::{AppError, AppResult};
use serde_json::json;

#[derive(Clone)]
pub struct EmailService {
    config: EmailConfig,
    client: reqwest::Client,
}

impl EmailService {
    pub fn new(config: EmailConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
        }
    }

    pub async fn send_contact_email(
        &self,
        email: &str,
        firstname: &str,
        lastname: &str,
        phone: &str,
        content: &str,
    ) -> AppResult<()> {
        let subject = format!("Contact Form Submission from {firstname} {lastname}");
        let body = format!(
            "You have received a new contact form submission.\n\n\
             Name: {firstname} {lastname}\n\
             Email: {email}\n\
             Phone: {phone}\n\n\
             Message:\n{content}"
        );

        self.send_email(&self.config.from_email, &self.config.to_email, &subject, &body)
            .await?;

        log::info!("Contact email sent from {email}");
        Ok(())
    }

    pub async fn send_subscribe_email(&self, email: &str) -> AppResult<()> {
        let subject = "New Newsletter Subscription";
        let body = format!("A new user has subscribed to the newsletter.\n\nEmail: {email}");

        self.send_email(&self.config.from_email, &self.config.to_email, subject, &body)
            .await?;

        log::info!("Subscribe email sent for {email}");
        Ok(())
    }

    async fn send_email(
        &self,
        from: &str,
        to: &str,
        subject: &str,
        body: &str,
    ) -> AppResult<()> {
        if self.config.resend_api_key.is_empty() {
            return Err(AppError::InternalError(
                "RESEND_API_KEY not configured".to_string(),
            ));
        }

        let payload = json!({
            "from": from,
            "to": [to],
            "subject": subject,
            "text": body,
        });

        let res = self
            .client
            .post("https://api.resend.com/emails")
            .header("Authorization", format!("Bearer {}", self.config.resend_api_key))
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await
            .map_err(|e| AppError::ExternalApiError(format!("Resend request error: {e}")))?;

        if !res.status().is_success() {
            let status = res.status();
            let text = res
                .text()
                .await
                .unwrap_or_else(|_| "unknown error".to_string());
            return Err(AppError::ExternalApiError(format!(
                "Resend API error {status}: {text}"
            )));
        }

        Ok(())
    }
}
