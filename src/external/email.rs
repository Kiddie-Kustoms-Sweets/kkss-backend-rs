use crate::config::SmtpConfig;
use crate::error::{AppError, AppResult};
use lettre::{
    message::Mailbox,
    transport::smtp::authentication::Credentials,
    AsyncSmtpTransport, AsyncTransport, Message,
};

#[derive(Clone)]
pub struct EmailService {
    config: SmtpConfig,
}

impl EmailService {
    pub fn new(config: SmtpConfig) -> Self {
        Self { config }
    }

    fn build_transport(&self) -> AppResult<AsyncSmtpTransport<lettre::Tokio1Executor>> {
        let creds = Credentials::new(
            self.config.username.clone(),
            self.config.password.clone(),
        );

        let transport = AsyncSmtpTransport::<lettre::Tokio1Executor>::relay(&self.config.host)
            .map_err(|e| AppError::InternalError(format!("SMTP relay error: {e}")))?
            .credentials(creds)
            .port(self.config.port)
            .build();

        Ok(transport)
    }

    pub async fn send_contact_email(
        &self,
        email: &str,
        firstname: &str,
        lastname: &str,
        phone: &str,
        content: &str,
    ) -> AppResult<()> {
        if self.config.username.is_empty() || self.config.password.is_empty() {
            return Err(AppError::InternalError(
                "SMTP username or password not configured".to_string(),
            ));
        }

        let from: Mailbox = self
            .config
            .from_email
            .parse()
            .map_err(|e| AppError::InternalError(format!("Invalid from email: {e}")))?;
        let to: Mailbox = self
            .config
            .to_email
            .parse()
            .map_err(|e| AppError::InternalError(format!("Invalid to email: {e}")))?;

        let subject = format!("Contact Form Submission from {firstname} {lastname}");
        let body = format!(
            "You have received a new contact form submission.\n\n\
             Name: {firstname} {lastname}\n\
             Email: {email}\n\
             Phone: {phone}\n\n\
             Message:\n{content}"
        );

        let message = Message::builder()
            .from(from)
            .to(to)
            .subject(subject)
            .body(body)
            .map_err(|e| AppError::InternalError(format!("Failed to build email: {e}")))?;

        let transport = self.build_transport()?;
        transport
            .send(message)
            .await
            .map_err(|e| AppError::ExternalApiError(format!("Failed to send email: {e}")))?;

        log::info!("Contact email sent from {email}");
        Ok(())
    }

    pub async fn send_subscribe_email(&self, email: &str) -> AppResult<()> {
        if self.config.username.is_empty() || self.config.password.is_empty() {
            return Err(AppError::InternalError(
                "SMTP username or password not configured".to_string(),
            ));
        }

        let from: Mailbox = self
            .config
            .from_email
            .parse()
            .map_err(|e| AppError::InternalError(format!("Invalid from email: {e}")))?;
        let to: Mailbox = self
            .config
            .to_email
            .parse()
            .map_err(|e| AppError::InternalError(format!("Invalid to email: {e}")))?;

        let subject = "New Newsletter Subscription";
        let body = format!("A new user has subscribed to the newsletter.\n\nEmail: {email}");

        let message = Message::builder()
            .from(from)
            .to(to)
            .subject(subject)
            .body(body)
            .map_err(|e| AppError::InternalError(format!("Failed to build email: {e}")))?;

        let transport = self.build_transport()?;
        transport
            .send(message)
            .await
            .map_err(|e| AppError::ExternalApiError(format!("Failed to send email: {e}")))?;

        log::info!("Subscribe email sent for {email}");
        Ok(())
    }
}
