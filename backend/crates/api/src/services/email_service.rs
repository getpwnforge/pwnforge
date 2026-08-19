use crate::config::{Config, EmailBackend, SmtpTls};
use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
    message::MultiPart,
    transport::smtp::authentication::Credentials,
};
use askama::Template;
use rust_i18n::t;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EmailError {
    #[error("email transport failed")]
    Transport(#[from] reqwest::Error),

    #[error("email rendering failed")]
    Render(#[from] askama::Error),

    #[error("email provider rejected the request: {0}")]
    Rejected(String),

    #[error("email transport failed")]
    Smtp(#[from] lettre::transport::smtp::Error),

    #[error("invalid email message")]
    Message(#[from] lettre::error::Error),

    #[error("invalid email address")]
    Address(#[from] lettre::address::AddressError),
}

#[derive(Clone, Copy, Debug)]
pub enum AlertKind {
    PasswordChanged,
    PasswordReset,
}

impl AlertKind {
    fn subject_key(self) -> &'static str {
        match self {
            AlertKind::PasswordChanged => "email.security_alert.password_changed.subject",
            AlertKind::PasswordReset => "email.security_alert.password_reset.subject",
        }
    }

    fn body_key(self) -> &'static str {
        match self {
            AlertKind::PasswordChanged => "email.security_alert.password_changed.body",
            AlertKind::PasswordReset => "email.security_alert.password_reset.body",
        }
    }
}

#[derive(Template)]
#[template(path = "password_reset.html.j2")]
struct PasswordResetHtml {
    locale: String,
    greeting: String,
    body: String,
    cta: String,
    link: String,
    expiry: String,
    ignore_notice: String,
    footer: String,
}

#[derive(Template)]
#[template(path = "password_reset.txt")]
struct PasswordResetText {
    greeting: String,
    body: String,
    cta: String,
    link: String,
    expiry: String,
    ignore_notice: String,
    footer: String,
}

#[derive(Template)]
#[template(path = "email_verification.html.j2")]
struct EmailVerificationHtml {
    locale: String,
    greeting: String,
    body: String,
    cta: String,
    link: String,
    expiry: String,
    ignore_notice: String,
    footer: String,
}

#[derive(Template)]
#[template(path = "email_verification.txt")]
struct EmailVerificationText {
    greeting: String,
    body: String,
    cta: String,
    link: String,
    expiry: String,
    ignore_notice: String,
    footer: String,
}

#[derive(Template)]
#[template(path = "security_alert.html.j2")]
struct SecurityAlertHtml {
    locale: String,
    greeting: String,
    body: String,
    label_when: String,
    label_ip: String,
    label_device: String,
    occurred_at: String,
    ip_address: String,
    user_agent: String,
    not_you: String,
    footer: String,
}

#[derive(Template)]
#[template(path = "security_alert.txt")]
struct SecurityAlertText {
    greeting: String,
    body: String,
    label_when: String,
    label_ip: String,
    label_device: String,
    occurred_at: String,
    ip_address: String,
    user_agent: String,
    not_you: String,
    footer: String,
}

pub async fn send_password_reset(
    config: &Config,
    to: &str,
    locale: &str,
    token: &str,
) -> Result<(), EmailError> {
    let link = format!("{}/auth/password/reset?token={token}", config.public_url);

    let greeting = t!("email.common.greeting", locale = locale).to_string();
    let body = t!("email.password_reset.body", locale = locale).to_string();
    let cta = t!("email.password_reset.cta", locale = locale).to_string();
    let expiry = t!("email.password_reset.expiry", locale = locale).to_string();
    let ignore_notice = t!("email.common.ignore_notice", locale = locale).to_string();
    let footer = t!("email.common.footer", locale = locale).to_string();
    let subject = t!("email.password_reset.subject", locale = locale).to_string();


    let html = PasswordResetHtml {
        locale: locale.to_string(),
        greeting: greeting.clone(),
        body: body.clone(),
        cta: cta.clone(),
        link: link.clone(),
        expiry: expiry.clone(),
        ignore_notice: ignore_notice.clone(),
        footer: footer.clone(),
    }
    .render()?;

    let text = PasswordResetText {
        greeting,
        body,
        cta,
        link,
        expiry,
        ignore_notice,
        footer,
    }
    .render()?;

    send(config, to, &subject, &html, &text).await
}

pub async fn send_email_verification(
    config: &Config,
    to: &str,
    locale: &str,
    token: &str,
) -> Result<(), EmailError> {
    // Same shape, link points at the verification page.
    let link = format!("{}/auth/email/verify?token={token}", config.public_url);

    let greeting = t!("email.common.greeting", locale = locale).to_string();
    let body = t!("email.email_verification.body", locale = locale).to_string();
    let cta = t!("email.email_verification.cta", locale = locale).to_string();
    let expiry = t!("email.email_verification.expiry", locale = locale).to_string();
    let ignore_notice = t!("email.common.ignore_notice", locale = locale).to_string();
    let footer = t!("email.common.footer", locale = locale).to_string();
    let subject = t!("email.email_verification.subject", locale = locale).to_string();

    let html = EmailVerificationHtml {
        locale: locale.to_string(),
        greeting: greeting.clone(),
        body: body.clone(),
        cta: cta.clone(),
        link: link.clone(),
        expiry: expiry.clone(),
        ignore_notice: ignore_notice.clone(),
        footer: footer.clone(),
    }
    .render()?;

    let text = EmailVerificationText {
        greeting,
        body,
        cta,
        link,
        expiry,
        ignore_notice,
        footer,
    }
    .render()?;

    send(config, to, &subject, &html, &text).await

}

pub async fn send_security_alert(
    config: &Config,
    to: &str,
    locale: &str,
    kind: AlertKind,
    occurred_at: &str,
    ip_address: Option<&str>,
    user_agent: Option<&str>,
) -> Result<(), EmailError> {
    let unknown = t!("email.common.unknown", locale = locale).to_string();

    let greeting = t!("email.common.greeting", locale = locale).to_string();
    let body = t!(kind.body_key(), locale = locale).to_string();
    let not_you = t!("email.security_alert.not_you", locale = locale).to_string();
    let footer = t!("email.common.footer", locale = locale).to_string();
    let subject = t!(kind.subject_key(), locale = locale).to_string();

    let label_when = t!("email.common.label_when", locale = locale).to_string();
    let label_ip = t!("email.common.label_ip", locale = locale).to_string();
    let label_device = t!("email.common.label_device", locale = locale).to_string();

    // Absent values get a translated placeholder rather than an empty cell:
    // a blank row reads as a rendering bug, not as missing information.
    let ip_address = ip_address.map(str::to_owned).unwrap_or_else(|| unknown.clone());
    let user_agent = user_agent.map(str::to_owned).unwrap_or_else(|| unknown.clone());

    let html = SecurityAlertHtml {
        locale: locale.to_owned(),
        greeting: greeting.clone(),
        body: body.clone(),
        label_when: label_when.clone(),
        label_ip: label_ip.clone(),
        label_device: label_device.clone(),
        occurred_at: occurred_at.to_owned(),
        ip_address: ip_address.clone(),
        user_agent: user_agent.clone(),
        not_you: not_you.clone(),
        footer: footer.clone(),
    }
    .render()?;

    let text = SecurityAlertText {
        greeting,
        body,
        label_when,
        label_ip,
        label_device,
        occurred_at: occurred_at.to_owned(),
        ip_address,
        user_agent,
        not_you,
        footer,
    }
    .render()?;

    send(config, to, &subject, &html, &text).await
}

/// Plain and untranslated on purpose: this only has to prove that the
/// transport works, and the operator has not picked a locale yet.
pub async fn send_test(config: &Config, to: &str) -> Result<(), EmailError> {
    send(
        config,
        to,
        "PwnForge test email",
        "<p>Your PwnForge instance can send email.</p>",
        "Your PwnForge instance can send email.",
    )
    .await
}

/// Console logs the message instead of sending it, so local development never
/// touches the provider and the link shows up in the container logs.
async fn send(
    config: &Config,
    to: &str,
    subject: &str,
    html: &str,
    text: &str,
) -> Result<(), EmailError> {
    match config.email_backend {
        EmailBackend::Console => {
            tracing::info!(to, subject, "email (console backend)");
            // println! rather than tracing for the body: the pretty formatter
            // wraps long lines, which makes the link painful to copy.
            println!("\n=== EMAIL to {to} ===\n{subject}\n\n{text}\n=== END ===\n");
            Ok(())
        }

        EmailBackend::Smtp => {
            let message = Message::builder()
                .from(config.email_from.parse()?)
                .to(to.parse()?)
                .subject(subject)
                // Both parts in one message: some clients show the text one,
                // and its absence hurts the spam score.
                .multipart(MultiPart::alternative_plain_html(
                    text.to_owned(),
                    html.to_owned(),
                ))?;

            smtp_transport(config)?.send(message).await?;
            Ok(())
        }

        EmailBackend::Resend => {
            let response = reqwest::Client::new()
                .post("https://api.resend.com/emails")
                .bearer_auth(&config.resend_api_key)
                .json(&serde_json::json!({
                    "from": config.email_from,
                    "to": to,
                    "subject": subject,
                    "html": html,
                    "text": text,
                }))
                .send()
                .await?;

            if !response.status().is_success() {
                return Err(EmailError::Rejected(response.text().await.unwrap_or_default()));
            }

            Ok(())
        }
    }
}

/// Built per call: cheap enough at this volume, and it keeps the transport out
/// of the shared state.
fn smtp_transport(config: &Config) -> Result<AsyncSmtpTransport<Tokio1Executor>, EmailError> {
    let mut builder = match config.smtp_tls {
        SmtpTls::Implicit => AsyncSmtpTransport::<Tokio1Executor>::relay(&config.smtp_host)?,
        SmtpTls::StartTls => AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&config.smtp_host)?,
        SmtpTls::None => AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&config.smtp_host),
    }
    .port(config.smtp_port);

    // A relay on the same host often accepts unauthenticated mail.
    if !config.smtp_username.is_empty() {
        builder = builder.credentials(Credentials::new(
            config.smtp_username.clone(),
            config.smtp_password.clone(),
        ));
    }

    Ok(builder.build())
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_reset_renders_in_both_languages() {
        let link = "https://example.test/auth/password/reset?token=abc123";

        for locale in ["en", "fr"] {
            let html = PasswordResetHtml {
                locale: locale.to_owned(),
                greeting: t!("email.common.greeting", locale = locale).to_string(),
                body: t!("email.password_reset.body", locale = locale).to_string(),
                cta: t!("email.password_reset.cta", locale = locale).to_string(),
                link: link.to_owned(),
                expiry: t!("email.password_reset.expiry", locale = locale).to_string(),
                ignore_notice: t!("email.common.ignore_notice", locale = locale).to_string(),
                footer: t!("email.common.footer", locale = locale).to_string(),
            }
            .render()
            .expect("template must render");

            assert!(html.contains(link), "link missing in {locale}");
            // Catches a variable that was never substituted.
            assert!(!html.contains("{{"), "unsubstituted placeholder in {locale}");
            assert!(html.contains(&format!(r#"lang="{locale}""#)));
        }
    }

    #[test]
    fn password_reset_text_carries_the_link() {
        let link = "https://example.test/auth/password/reset?token=abc123";

        let text = PasswordResetText {
            greeting: t!("email.common.greeting", locale = "en").to_string(),
            body: t!("email.password_reset.body", locale = "en").to_string(),
            cta: t!("email.password_reset.cta", locale = "en").to_string(),
            link: link.to_owned(),
            expiry: t!("email.password_reset.expiry", locale = "en").to_string(),
            ignore_notice: t!("email.common.ignore_notice", locale = "en").to_string(),
            footer: t!("email.common.footer", locale = "en").to_string(),
        }
        .render()
        .expect("template must render");

        assert!(text.contains(link));
    }
}
