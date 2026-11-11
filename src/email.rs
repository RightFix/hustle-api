use lettre::{
    message::Mailbox, transport::smtp::authentication::Credentials, AsyncSmtpTransport,
    AsyncTransport, Message, Tokio1Executor,
};

use crate::{
    config::Config,
    error::{AppError, AppResult},
};

fn transport(config: &Config) -> Result<AsyncSmtpTransport<Tokio1Executor>, String> {
    if config.smtp_user.is_empty() {
        Ok(
            AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&config.smtp_host)
                .port(config.smtp_port)
                .build(),
        )
    } else {
        Ok(
            AsyncSmtpTransport::<Tokio1Executor>::relay(&config.smtp_host)
                .map_err(|err| err.to_string())?
                .port(config.smtp_port)
                .credentials(Credentials::new(
                    config.smtp_user.clone(),
                    config.smtp_pass.clone(),
                ))
                .build(),
        )
    }
}

async fn send(config: &Config, to: &str, subject: &str, body: String) -> AppResult<()> {
    let email = Message::builder()
        .from(config.smtp_from.parse::<Mailbox>().map_err(|err| {
            tracing::error!(error = %err, "invalid SMTP_FROM address");
            AppError::Internal
        })?)
        .to(to.parse::<Mailbox>().map_err(|err| {
            tracing::error!(error = %err, "invalid recipient address");
            AppError::Internal
        })?)
        .subject(subject)
        .body(body)
        .map_err(|err| {
            tracing::error!(error = %err, "failed to build email");
            AppError::Internal
        })?;

    let mailer = transport(config).map_err(|err| {
        tracing::error!(error = %err, "failed to build SMTP transport");
        AppError::Internal
    })?;
    mailer.send(email).await.map_err(|err| {
        tracing::error!(error = %err, "failed to send email");
        AppError::Internal
    })?;
    Ok(())
}

pub async fn send_reset_email(config: &Config, to: &str, token: &str) -> AppResult<()> {
    let link = format!(
        "{}/reset-password?token={}",
        config.frontend_url.trim_end_matches('/'),
        token
    );
    let body = format!(
        "Someone requested a password reset for your Hustle account.\\n\\n\\
         Open this link to choose a new password (valid for 60 minutes):\\n{link}\\n\\n\\
         If you did not request this, you can safely ignore this email."
    );
    send(config, to, "Reset your Hustle password", body).await
}

pub async fn send_verification_email(config: &Config, to: &str, token: &str) -> AppResult<()> {
    let link = format!(
        "{}/verify-email?token={}",
        config.frontend_url.trim_end_matches('/'),
        token
    );
    let body = format!(
        "Welcome to Hustle!\\n\\n\\
         Open this link to verify your email (valid for 48 hours):\\n{link}\\n\\n\\
         If you did not create this account, you can safely ignore this email."
    );
    send(config, to, "Verify your Hustle email", body).await
}

pub async fn send_message_email(
    config: &Config,
    to: &str,
    sender_username: &str,
    conversation_id: i32,
) -> AppResult<()> {
    let link = format!(
        "{}/messages/{conversation_id}",
        config.frontend_url.trim_end_matches('/')
    );
    let body = format!(
        "@{sender_username} sent you a message on Hustle.\\n\\n\\
         Open this link to reply:\\n{link}"
    );
    send(
        config,
        to,
        &format!("New message from @{sender_username}"),
        body,
    )
    .await
}
