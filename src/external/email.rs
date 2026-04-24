use crate::config::EmailConfig;
use crate::error::{AppError, AppResult};
use resend_rs::types::CreateEmailBaseOptions;
use resend_rs::Resend;

#[derive(Clone)]
pub struct EmailService {
    config: EmailConfig,
    client: Resend,
}

impl EmailService {
    pub fn new(config: EmailConfig) -> Self {
        let client = Resend::new(&config.resend_api_key);
        Self { config, client }
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

        let opts = CreateEmailBaseOptions::new(from, [to], subject).with_text(body);

        self.client
            .emails
            .send(opts)
            .await
            .map_err(|e| AppError::ExternalApiError(format!("Resend error: {e}")))?;

        Ok(())
    }
}
