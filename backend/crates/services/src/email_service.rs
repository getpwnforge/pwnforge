use crate::config::{Config, EmailBackend, SmtpTls};
use askama::Template;
use base64::Engine as _;
use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
    message::{Attachment, MultiPart, header::ContentType},
    transport::smtp::authentication::Credentials,
};
use rust_i18n::t;
use thiserror::Error;

/// The logo travels inside the message rather than being fetched from
/// `PUBLIC_URL`. A remote `<img>` only ever loads when the instance is
/// reachable from the *reader's* mail client, which rules out every localhost
/// setup and every instance on a private network — the common case for
/// self-hosted software. Embedded, it works everywhere the reader allows
/// images at all.
const LOGO_BYTES: &[u8] = include_bytes!("../assets/email-icon.png");

/// Referenced by the layout as `src="cid:pwnforge-logo"`. Bare here, angle
/// brackets only in the `Content-ID` header, which lettre adds itself.
const LOGO_CID: &str = "pwnforge-logo";

/// `multipart/related` wrapping a `multipart/alternative`: the alternative
/// holds the text and HTML versions, the related part holds what the HTML
/// refers to. Nesting them the other way round leaves clients showing the
/// logo as a bare attachment.
fn body_with_logo(text: &str, html: &str) -> MultiPart {
    MultiPart::related()
        .multipart(MultiPart::alternative_plain_html(
            text.to_owned(),
            html.to_owned(),
        ))
        .singlepart(
            Attachment::new_inline(LOGO_CID.to_owned()).body(LOGO_BYTES.to_vec(), logo_mime()),
        )
}

/// `ContentType::parse` is fallible on arbitrary input; this input is a
/// literal, so the failure branch is unreachable and unwrapping it here keeps
/// the caller free of a `Result` that can never be `Err`.
fn logo_mime() -> ContentType {
    ContentType::parse("image/png").expect("image/png is a valid MIME type")
}

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

impl EmailError {
    /// The mail service did not take the message: unreachable, down or
    /// refusing it. Not a message that could not be built, which is a bug.
    pub fn is_delivery_failure(&self) -> bool {
        matches!(self, Self::Transport(_) | Self::Smtp(_) | Self::Rejected(_))
    }
}

/// Whether `address` makes a mailbox, parsed as `send` parses every address
/// it is given. Stricter than the `email` validator on the request bodies:
/// `a..b@example.test` and `a@[127.0.0.1]` pass that one and fail this.
pub fn is_valid_mailbox(address: &str) -> bool {
    address.parse::<lettre::message::Mailbox>().is_ok()
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

    fn title_key(self) -> &'static str {
        match self {
            AlertKind::PasswordChanged => "email.security_alert.password_changed.title",
            AlertKind::PasswordReset => "email.security_alert.password_reset.title",
        }
    }

    fn preheader_key(self) -> &'static str {
        match self {
            AlertKind::PasswordChanged => "email.security_alert.password_changed.preheader",
            AlertKind::PasswordReset => "email.security_alert.password_reset.preheader",
        }
    }

    fn body_key(self) -> &'static str {
        match self {
            AlertKind::PasswordChanged => "email.security_alert.password_changed.body",
            AlertKind::PasswordReset => "email.security_alert.password_reset.body",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum AccountEventKind {
    Suspended,
    Unsuspended,
    Deleted,
    DeletionScheduled,
    DeletionCancelled,
    PromotedAdmin,
    DemotedAdmin,
}

impl AccountEventKind {
    fn subject_key(self) -> &'static str {
        match self {
            Self::Suspended => "email.account_notice.suspended.subject",
            Self::Unsuspended => "email.account_notice.unsuspended.subject",
            Self::Deleted => "email.account_notice.deleted.subject",
            Self::DeletionScheduled => "email.account_notice.deletion_scheduled.subject",
            Self::DeletionCancelled => "email.account_notice.deletion_cancelled.subject",
            Self::PromotedAdmin => "email.account_notice.promoted_admin.subject",
            Self::DemotedAdmin => "email.account_notice.demoted_admin.subject",
        }
    }
    fn title_key(self) -> &'static str {
        match self {
            Self::Suspended => "email.account_notice.suspended.title",
            Self::Unsuspended => "email.account_notice.unsuspended.title",
            Self::Deleted => "email.account_notice.deleted.title",
            Self::DeletionScheduled => "email.account_notice.deletion_scheduled.title",
            Self::DeletionCancelled => "email.account_notice.deletion_cancelled.title",
            Self::PromotedAdmin => "email.account_notice.promoted_admin.title",
            Self::DemotedAdmin => "email.account_notice.demoted_admin.title",
        }
    }
    fn preheader_key(self) -> &'static str {
        match self {
            Self::Suspended => "email.account_notice.suspended.preheader",
            Self::Unsuspended => "email.account_notice.unsuspended.preheader",
            Self::Deleted => "email.account_notice.deleted.preheader",
            Self::DeletionScheduled => "email.account_notice.deletion_scheduled.preheader",
            Self::DeletionCancelled => "email.account_notice.deletion_cancelled.preheader",
            Self::PromotedAdmin => "email.account_notice.promoted_admin.preheader",
            Self::DemotedAdmin => "email.account_notice.demoted_admin.preheader",
        }
    }
    fn body_key(self) -> &'static str {
        match self {
            Self::Suspended => "email.account_notice.suspended.body",
            Self::Unsuspended => "email.account_notice.unsuspended.body",
            Self::Deleted => "email.account_notice.deleted.body",
            Self::DeletionScheduled => "email.account_notice.deletion_scheduled.body",
            Self::DeletionCancelled => "email.account_notice.deletion_cancelled.body",
            Self::PromotedAdmin => "email.account_notice.promoted_admin.body",
            Self::DemotedAdmin => "email.account_notice.demoted_admin.body",
        }
    }
}

#[derive(Clone)]
pub struct DetailRow {
    pub label: String,
    pub value: String,
}

pub enum SuspensionDuration {
    Until(chrono::DateTime<chrono::FixedOffset>),
    Permanent,
}

/// `until` only makes sense for `Suspended`; the caller decides what to pass
/// rather than the template guessing from `kind`.
pub struct AccountEventDetails {
    pub username: String,
    pub until: Option<SuspensionDuration>,
    pub purge_date: Option<chrono::DateTime<chrono::FixedOffset>>,
}

/// The three lines every message ends with.
///
/// Grouped because the layout renders all of them, and in askama a forgotten
/// field is a compile error rather than a blank line at the bottom of an email.
struct Footer {
    text: String,
    instance: String,
    note: String,
}

impl Footer {
    fn new(locale: &str) -> Self {
        Self {
            text: t!("email.common.footer", locale = locale).to_string(),
            instance: t!("email.common.footer_instance", locale = locale).to_string(),
            note: t!("email.common.footer_note", locale = locale).to_string(),
        }
    }
}

#[derive(Template)]
#[template(path = "password_reset.html.j2")]
struct PasswordResetHtml {
    locale: String,
    title: String,
    preheader: String,
    greeting: String,
    body: String,
    cta: String,
    link: String,
    fallback_notice: String,
    expiry: String,
    ignore_notice: String,
    footer: String,
    footer_instance: String,
    footer_note: String,
    public_url: String,
}

#[derive(Template)]
#[template(path = "password_reset.txt")]
struct PasswordResetText {
    title: String,
    greeting: String,
    body: String,
    cta: String,
    link: String,
    expiry: String,
    ignore_notice: String,
    footer: String,
    footer_instance: String,
    footer_note: String,
    public_url: String,
}

#[derive(Template)]
#[template(path = "email_verification.html.j2")]
struct EmailVerificationHtml {
    locale: String,
    title: String,
    preheader: String,
    greeting: String,
    body: String,
    cta: String,
    link: String,
    fallback_notice: String,
    expiry: String,
    ignore_notice: String,
    footer: String,
    footer_instance: String,
    footer_note: String,
    public_url: String,
}

#[derive(Template)]
#[template(path = "email_verification.txt")]
struct EmailVerificationText {
    title: String,
    greeting: String,
    body: String,
    cta: String,
    link: String,
    expiry: String,
    ignore_notice: String,
    footer: String,
    footer_instance: String,
    footer_note: String,
    public_url: String,
}

#[derive(Template)]
#[template(path = "invitation.html.j2")]
struct InvitationHtml {
    locale: String,
    title: String,
    preheader: String,
    greeting: String,
    body: String,
    cta: String,
    link: String,
    fallback_notice: String,
    expiry: String,
    ignore_notice: String,
    footer: String,
    footer_instance: String,
    footer_note: String,
    public_url: String,
}

#[derive(Template)]
#[template(path = "invitation.txt")]
struct InvitationText {
    title: String,
    greeting: String,
    body: String,
    cta: String,
    link: String,
    expiry: String,
    ignore_notice: String,
    footer: String,
    footer_instance: String,
    footer_note: String,
    public_url: String,
}

#[derive(Template)]
#[template(path = "security_alert.html.j2")]
struct SecurityAlertHtml {
    locale: String,
    title: String,
    preheader: String,
    greeting: String,
    body: String,
    label_when: String,
    label_ip: String,
    label_device: String,
    occurred_at: String,
    ip_address: String,
    user_agent: String,
    no_action: String,
    not_you: String,
    footer: String,
    footer_instance: String,
    footer_note: String,
    public_url: String,
}

#[derive(Template)]
#[template(path = "security_alert.txt")]
struct SecurityAlertText {
    title: String,
    greeting: String,
    body: String,
    label_when: String,
    label_ip: String,
    label_device: String,
    occurred_at: String,
    ip_address: String,
    user_agent: String,
    no_action: String,
    not_you: String,
    footer: String,
    footer_instance: String,
    footer_note: String,
    public_url: String,
}

#[derive(Template)]
#[template(path = "account_notice.html.j2")]
struct AccountNoticeHtml {
    locale: String,
    title: String,
    preheader: String,
    greeting: String,
    body: String,
    details: Vec<DetailRow>,
    support_email: Option<String>,
    label_contact: String,
    footer: String,
    footer_instance: String,
    footer_note: String,
    public_url: String,
}

#[derive(Template)]
#[template(path = "account_notice.txt")]
struct AccountNoticeText {
    title: String,
    greeting: String,
    body: String,
    details: Vec<DetailRow>,
    support_email: Option<String>,
    label_contact: String,
    footer: String,
    footer_instance: String,
    footer_note: String,
    public_url: String,
}

#[derive(Template)]
#[template(path = "contact_notification.html.j2")]
struct ContactNotificationHtml {
    locale: String,
    title: String,
    preheader: String,
    footer: String,
    footer_instance: String,
    footer_note: String,
    public_url: String,
    name: String,
    email: String,
    category: String,
    message: String,
}

#[derive(Template)]
#[template(path = "contact_notification.txt")]
struct ContactNotificationText {
    name: String,
    email: String,
    category: String,
    message: String,
}

#[derive(Template)]
#[template(path = "contact_confirmation.html.j2")]
struct ContactConfirmationHtml {
    locale: String,
    title: String,
    preheader: String,
    greeting: String,
    body: String,
    reference: String,
    footer: String,
    footer_instance: String,
    footer_note: String,
    public_url: String,
}

#[derive(Template)]
#[template(path = "contact_confirmation.txt")]
struct ContactConfirmationText {
    title: String,
    greeting: String,
    body: String,
    reference: String,
    footer: String,
    footer_instance: String,
    footer_note: String,
    public_url: String,
}

#[derive(Template)]
#[template(path = "test_email.html.j2")]
struct TestEmailHtml {
    locale: String,
    title: String,
    preheader: String,
    footer: String,
    footer_instance: String,
    footer_note: String,
    public_url: String,
}

#[derive(Template)]
#[template(path = "test_email.txt")]
struct TestEmailText {
    title: String,
    footer_instance: String,
    public_url: String,
}

/// A link to a page of the app carrying a token, in the fragment: what
/// follows `#` never leaves the browser, so the token stays out of the
/// access logs of every server and proxy the page load goes through. The
/// page reads it, then drops it from the address bar and the history.
fn token_link(public_url: &str, path: &str, token: &str) -> String {
    format!("{public_url}/{path}#token={token}")
}

fn reset_link(public_url: &str, token: &str) -> String {
    token_link(public_url, "auth/password/reset", token)
}

fn verification_link(public_url: &str, token: &str) -> String {
    token_link(public_url, "auth/email/verify", token)
}

pub async fn send_password_reset(
    config: &Config,
    to: &str,
    locale: &str,
    token: &str,
) -> Result<(), EmailError> {
    let link = reset_link(&config.public_url, token);

    let title = t!("email.password_reset.title", locale = locale).to_string();
    let preheader = t!("email.password_reset.preheader", locale = locale).to_string();
    let greeting = t!("email.common.greeting", locale = locale).to_string();
    let body = t!("email.password_reset.body", locale = locale).to_string();
    let cta = t!("email.password_reset.cta", locale = locale).to_string();
    let fallback_notice = t!("email.common.fallback_notice", locale = locale).to_string();
    let expiry = t!("email.password_reset.expiry", locale = locale).to_string();
    let ignore_notice = t!("email.common.ignore_notice", locale = locale).to_string();
    let subject = t!("email.password_reset.subject", locale = locale).to_string();
    let footer = Footer::new(locale);

    let html = PasswordResetHtml {
        locale: locale.to_string(),
        title: title.clone(),
        preheader,
        greeting: greeting.clone(),
        body: body.clone(),
        cta: cta.clone(),
        link: link.clone(),
        fallback_notice,
        expiry: expiry.clone(),
        ignore_notice: ignore_notice.clone(),
        footer: footer.text.clone(),
        footer_instance: footer.instance.clone(),
        footer_note: footer.note.clone(),
        public_url: config.public_url.clone(),
    }
    .render()?;

    let text = PasswordResetText {
        title,
        greeting,
        body,
        cta,
        link,
        expiry,
        ignore_notice,
        footer: footer.text,
        footer_instance: footer.instance,
        footer_note: footer.note,
        public_url: config.public_url.clone(),
    }
    .render()?;

    send(config, to, &subject, &html, &text, None).await
}

pub async fn send_email_verification(
    config: &Config,
    to: &str,
    locale: &str,
    token: &str,
) -> Result<(), EmailError> {
    // Same shape, link points at the verification page.
    let link = verification_link(&config.public_url, token);

    let title = t!("email.email_verification.title", locale = locale).to_string();
    let preheader = t!("email.email_verification.preheader", locale = locale).to_string();
    let greeting = t!("email.common.greeting", locale = locale).to_string();
    let body = t!("email.email_verification.body", locale = locale).to_string();
    let cta = t!("email.email_verification.cta", locale = locale).to_string();
    let fallback_notice = t!("email.common.fallback_notice", locale = locale).to_string();
    let expiry = t!("email.email_verification.expiry", locale = locale).to_string();
    let ignore_notice = t!("email.common.ignore_notice", locale = locale).to_string();
    let subject = t!("email.email_verification.subject", locale = locale).to_string();
    let footer = Footer::new(locale);

    let html = EmailVerificationHtml {
        locale: locale.to_string(),
        title: title.clone(),
        preheader,
        greeting: greeting.clone(),
        body: body.clone(),
        cta: cta.clone(),
        link: link.clone(),
        fallback_notice,
        expiry: expiry.clone(),
        ignore_notice: ignore_notice.clone(),
        footer: footer.text.clone(),
        footer_instance: footer.instance.clone(),
        footer_note: footer.note.clone(),
        public_url: config.public_url.clone(),
    }
    .render()?;

    let text = EmailVerificationText {
        title,
        greeting,
        body,
        cta,
        link,
        expiry,
        ignore_notice,
        footer: footer.text,
        footer_instance: footer.instance,
        footer_note: footer.note,
        public_url: config.public_url.clone(),
    }
    .render()?;

    send(config, to, &subject, &html, &text, None).await
}

/// What an invitation mail invites to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvitationKind {
    Team,
    Workspace,
}

impl InvitationKind {
    /// The path segment of the page that opens the invitation.
    fn segment(self) -> &'static str {
        match self {
            InvitationKind::Team => "team",
            InvitationKind::Workspace => "workspace",
        }
    }
}

/// The one invitation mail, for teams and workspaces alike, and the same
/// whether an account holds `to` or not: it asks the reader to sign in, or
/// to create an account with this address first, and links to the
/// invitation itself.
///
/// The link carries the token in clear, as the verification and reset links
/// do; the page posts it back in a body, and it only ever opens the
/// invitation to the account it names.
pub async fn send_invitation(
    config: &Config,
    to: &str,
    locale: &str,
    kind: InvitationKind,
    name: &str,
    token: &str,
    expires_at: chrono::DateTime<chrono::Utc>,
) -> Result<(), EmailError> {
    let message = invitation_message(&config.public_url, locale, kind, name, token, expires_at)?;
    send(
        config,
        to,
        &message.subject,
        &message.html,
        &message.text,
        None,
    )
    .await
}

struct RenderedMessage {
    subject: String,
    html: String,
    text: String,
}

fn invitation_message(
    public_url: &str,
    locale: &str,
    kind: InvitationKind,
    name: &str,
    token: &str,
    expires_at: chrono::DateTime<chrono::Utc>,
) -> Result<RenderedMessage, askama::Error> {
    let link = token_link(
        public_url,
        &format!("invitations/{}", kind.segment()),
        token,
    );
    let (subject_key, preheader_key, body_key) = match kind {
        InvitationKind::Team => (
            "email.invitation.subject_team",
            "email.invitation.preheader_team",
            "email.invitation.body_team",
        ),
        InvitationKind::Workspace => (
            "email.invitation.subject_workspace",
            "email.invitation.preheader_workspace",
            "email.invitation.body_workspace",
        ),
    };

    let date = expires_at.format("%Y-%m-%d %H:%M").to_string();
    let title = t!(subject_key, locale = locale).to_string();
    let subject = title.clone();
    let preheader = t!(preheader_key, locale = locale, name = name).to_string();
    let greeting = t!("email.common.greeting", locale = locale).to_string();
    let body = t!(body_key, locale = locale, name = name).to_string();
    let cta = t!("email.invitation.cta", locale = locale).to_string();
    let fallback_notice = t!("email.common.fallback_notice", locale = locale).to_string();
    let expiry = t!("email.invitation.expiry", locale = locale, date = date).to_string();
    let ignore_notice = t!("email.invitation.ignore_notice", locale = locale).to_string();
    let footer = Footer::new(locale);

    let html = InvitationHtml {
        locale: locale.to_string(),
        title: title.clone(),
        preheader,
        greeting: greeting.clone(),
        body: body.clone(),
        cta: cta.clone(),
        link: link.clone(),
        fallback_notice,
        expiry: expiry.clone(),
        ignore_notice: ignore_notice.clone(),
        footer: footer.text.clone(),
        footer_instance: footer.instance.clone(),
        footer_note: footer.note.clone(),
        public_url: public_url.to_owned(),
    }
    .render()?;

    let text = InvitationText {
        title,
        greeting,
        body,
        cta,
        link,
        expiry,
        ignore_notice,
        footer: footer.text,
        footer_instance: footer.instance,
        footer_note: footer.note,
        public_url: public_url.to_owned(),
    }
    .render()?;

    Ok(RenderedMessage {
        subject,
        html,
        text,
    })
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

    let title = t!(kind.title_key(), locale = locale).to_string();
    let preheader = t!(kind.preheader_key(), locale = locale).to_string();
    let greeting = t!("email.common.greeting", locale = locale).to_string();
    let body = t!(kind.body_key(), locale = locale).to_string();
    let no_action = t!("email.security_alert.no_action", locale = locale).to_string();
    let not_you = t!("email.security_alert.not_you", locale = locale).to_string();
    let subject = t!(kind.subject_key(), locale = locale).to_string();
    let footer = Footer::new(locale);

    let label_when = t!("email.common.label_when", locale = locale).to_string();
    let label_ip = t!("email.common.label_ip", locale = locale).to_string();
    let label_device = t!("email.common.label_device", locale = locale).to_string();

    // Absent values get a translated placeholder rather than an empty cell:
    // a blank row reads as a rendering bug, not as missing information.
    let ip_address = ip_address
        .map(str::to_owned)
        .unwrap_or_else(|| unknown.clone());
    let user_agent = user_agent
        .map(str::to_owned)
        .unwrap_or_else(|| unknown.clone());

    let html = SecurityAlertHtml {
        locale: locale.to_owned(),
        title: title.clone(),
        preheader,
        greeting: greeting.clone(),
        body: body.clone(),
        label_when: label_when.clone(),
        label_ip: label_ip.clone(),
        label_device: label_device.clone(),
        occurred_at: occurred_at.to_owned(),
        ip_address: ip_address.clone(),
        user_agent: user_agent.clone(),
        no_action: no_action.clone(),
        not_you: not_you.clone(),
        footer: footer.text.clone(),
        footer_instance: footer.instance.clone(),
        footer_note: footer.note.clone(),
        public_url: config.public_url.clone(),
    }
    .render()?;

    let text = SecurityAlertText {
        title,
        greeting,
        body,
        label_when,
        label_ip,
        label_device,
        occurred_at: occurred_at.to_owned(),
        ip_address,
        user_agent,
        no_action,
        not_you,
        footer: footer.text,
        footer_instance: footer.instance,
        footer_note: footer.note,
        public_url: config.public_url.clone(),
    }
    .render()?;

    send(config, to, &subject, &html, &text, None).await
}

pub async fn send_account_notice(
    config: &Config,
    to: &str,
    locale: &str,
    kind: AccountEventKind,
    details: AccountEventDetails,
) -> Result<(), EmailError> {
    let title = t!(kind.title_key(), locale = locale).to_string();
    let preheader = t!(kind.preheader_key(), locale = locale).to_string();
    let greeting = t!("email.common.greeting", locale = locale).to_string();
    let body = t!(kind.body_key(), locale = locale).to_string();
    let subject = t!(kind.subject_key(), locale = locale).to_string();
    let label_contact = t!("email.account_notice.label_contact", locale = locale).to_string();
    let footer = Footer::new(locale);

    let mut rows = vec![DetailRow {
        label: t!("email.account_notice.label_account", locale = locale).to_string(),
        value: details.username,
    }];

    if let Some(until) = details.until {
        let value = match until {
            SuspensionDuration::Permanent => {
                t!("email.account_notice.permanent", locale = locale).to_string()
            }
            SuspensionDuration::Until(dt) => dt.format("%Y-%m-%d %H:%M UTC").to_string(),
        };
        rows.push(DetailRow {
            label: t!("email.account_notice.label_until", locale = locale).to_string(),
            value,
        });
    }

    if let Some(purge_date) = details.purge_date {
        rows.push(DetailRow {
            label: t!("email.account_notice.label_purge_date", locale = locale).to_string(),
            value: purge_date.format("%Y-%m-%d").to_string(),
        });
    }

    let html = AccountNoticeHtml {
        locale: locale.to_owned(),
        title: title.clone(),
        preheader,
        greeting: greeting.clone(),
        body: body.clone(),
        details: rows.clone(),
        support_email: config.support_email.clone(),
        label_contact: label_contact.clone(),
        footer: footer.text.clone(),
        footer_instance: footer.instance.clone(),
        footer_note: footer.note.clone(),
        public_url: config.public_url.clone(),
    }
    .render()?;

    let text = AccountNoticeText {
        title,
        greeting,
        body,
        details: rows,
        support_email: config.support_email.clone(),
        label_contact,
        footer: footer.text,
        footer_instance: footer.instance,
        footer_note: footer.note,
        public_url: config.public_url.clone(),
    }
    .render()?;

    send(config, to, &subject, &html, &text, None).await
}

/// What a sign-up notice tells the holder of an address someone just tried
/// to register with. Sent in place of an error, so that registration never
/// says whether an address has an account (see `auth_service::register`).
pub enum SignupNotice {
    /// The address belongs to an account that verified it: sign in, or
    /// reset the password.
    ExistingAccount,
    /// The address is held by a registration nobody confirmed, which the
    /// cleanup task deletes after `deleted_after`. Never with that
    /// registration's verification link: whoever reads this owns the
    /// mailbox, not that account.
    PendingRegistration {
        deleted_after: chrono::DateTime<chrono::Utc>,
    },
}

#[derive(Clone)]
struct NoticeLink {
    label: String,
    href: String,
}

#[derive(Template)]
#[template(path = "signup_notice.html.j2")]
struct SignupNoticeHtml {
    locale: String,
    title: String,
    preheader: String,
    greeting: String,
    body: String,
    detail: Option<String>,
    links: Vec<NoticeLink>,
    closing: String,
    footer: String,
    footer_instance: String,
    footer_note: String,
    public_url: String,
}

#[derive(Template)]
#[template(path = "signup_notice.txt")]
struct SignupNoticeText {
    title: String,
    greeting: String,
    body: String,
    detail: Option<String>,
    links: Vec<NoticeLink>,
    closing: String,
    footer: String,
    footer_instance: String,
    footer_note: String,
    public_url: String,
}

/// Renders a sign-up notice: subject, HTML and text.
fn render_signup_notice(
    public_url: &str,
    locale: &str,
    notice: &SignupNotice,
) -> Result<(String, String, String), EmailError> {
    let key = match notice {
        SignupNotice::ExistingAccount => "existing",
        SignupNotice::PendingRegistration { .. } => "pending",
    };
    let text_of =
        |name: &str| t!(format!("email.signup_notice.{key}.{name}"), locale = locale).to_string();

    let (detail, links) = match notice {
        SignupNotice::ExistingAccount => (
            None,
            vec![
                NoticeLink {
                    label: text_of("sign_in"),
                    href: format!("{public_url}/login"),
                },
                NoticeLink {
                    label: text_of("forgot_password"),
                    href: format!("{public_url}/forgot-password"),
                },
            ],
        ),
        SignupNotice::PendingRegistration { deleted_after } => {
            let date = deleted_after.format("%Y-%m-%d %H:%M UTC").to_string();
            (
                Some(
                    t!(
                        "email.signup_notice.pending.detail",
                        locale = locale,
                        date = date
                    )
                    .to_string(),
                ),
                Vec::new(),
            )
        }
    };

    let title = text_of("title");
    let greeting = t!("email.common.greeting", locale = locale).to_string();
    let body = text_of("body");
    let closing = text_of("closing");
    let footer = Footer::new(locale);

    let html = SignupNoticeHtml {
        locale: locale.to_owned(),
        title: title.clone(),
        preheader: text_of("preheader"),
        greeting: greeting.clone(),
        body: body.clone(),
        detail: detail.clone(),
        links: links.clone(),
        closing: closing.clone(),
        footer: footer.text.clone(),
        footer_instance: footer.instance.clone(),
        footer_note: footer.note.clone(),
        public_url: public_url.to_owned(),
    }
    .render()?;

    let text = SignupNoticeText {
        title,
        greeting,
        body,
        detail,
        links,
        closing,
        footer: footer.text,
        footer_instance: footer.instance,
        footer_note: footer.note,
        public_url: public_url.to_owned(),
    }
    .render()?;

    Ok((text_of("subject"), html, text))
}

pub async fn send_signup_notice(
    config: &Config,
    to: &str,
    locale: &str,
    notice: SignupNotice,
) -> Result<(), EmailError> {
    let (subject, html, text) = render_signup_notice(&config.public_url, locale, &notice)?;
    send(config, to, &subject, &html, &text, None).await
}

/// Untranslated on purpose: this only has to prove that the transport works,
/// and the operator has not picked a locale yet. It still goes through the
/// shared layout — it is the first email an instance ever sends, and the one
/// the operator judges the setup on.
pub async fn send_test(config: &Config, to: &str) -> Result<(), EmailError> {
    let title = t!("email.test.title", locale = "en").to_string();
    let footer = Footer::new("en");

    let html = TestEmailHtml {
        locale: "en".to_owned(),
        title: title.clone(),
        preheader: t!("email.test.preheader", locale = "en").to_string(),
        footer: footer.text,
        footer_instance: footer.instance.clone(),
        footer_note: footer.note,
        public_url: config.public_url.clone(),
    }
    .render()?;

    let text = TestEmailText {
        title,
        footer_instance: footer.instance,
        public_url: config.public_url.clone(),
    }
    .render()?;

    send(config, to, "PwnForge test email", &html, &text, None).await
}

fn contact_subject(reference: &str, category: &str) -> String {
    format!("[{reference}] - {category}")
}

/// Internal notification to the instance operator — always English, no
/// locale/i18n. Reply-To is the visitor's own address, so replying from the
/// operator's inbox reaches them directly.
pub async fn send_contact_notification(
    config: &Config,
    request: &domain::dto::contact::ContactRequest,
    reference: &str,
) -> Result<(), EmailError> {
    let subject = contact_subject(reference, request.category.as_str());
    let footer = Footer::new("en");

    let html = ContactNotificationHtml {
        locale: "en".to_owned(),
        title: "New contact form submission".to_owned(),
        preheader: "A visitor submitted the contact form.".to_owned(),
        footer: footer.text.clone(),
        footer_instance: footer.instance.clone(),
        footer_note: footer.note.clone(),
        public_url: config.public_url.clone(),
        name: request.name.clone(),
        email: request.email.clone(),
        category: request.category.as_str().to_owned(),
        message: request.message.clone(),
    }
    .render()?;

    let text = ContactNotificationText {
        name: request.name.clone(),
        email: request.email.clone(),
        category: request.category.as_str().to_owned(),
        message: request.message.clone(),
    }
    .render()?;

    let to = config
        .support_email
        .as_deref()
        .unwrap_or(&config.email_from);
    send(config, to, &subject, &html, &text, Some(&request.email)).await
}

/// Confirmation to the visitor. Reply-To is the operator's support address,
/// so a reply lands back in the same inbox as the original notification —
/// same subject on both emails keeps them threaded together.
pub async fn send_contact_confirmation(
    config: &Config,
    request: &domain::dto::contact::ContactRequest,
    reference: &str,
) -> Result<(), EmailError> {
    let subject = contact_subject(reference, request.category.as_str());
    let title = t!("email.contact_confirmation.title", locale = &request.locale).to_string();
    let preheader = t!(
        "email.contact_confirmation.preheader",
        locale = &request.locale
    )
    .to_string();
    let greeting = t!("email.common.greeting", locale = &request.locale).to_string();
    let body = t!("email.contact_confirmation.body", locale = &request.locale).to_string();
    let footer = Footer::new(&request.locale);

    let html = ContactConfirmationHtml {
        locale: request.locale.clone(),
        title: title.clone(),
        preheader,
        greeting: greeting.clone(),
        body: body.clone(),
        reference: reference.to_owned(),
        footer: footer.text.clone(),
        footer_instance: footer.instance.clone(),
        footer_note: footer.note.clone(),
        public_url: config.public_url.clone(),
    }
    .render()?;

    let text = ContactConfirmationText {
        title,
        greeting,
        body,
        reference: reference.to_owned(),
        footer: footer.text,
        footer_instance: footer.instance,
        footer_note: footer.note,
        public_url: config.public_url.clone(),
    }
    .render()?;

    let reply_to = config
        .support_email
        .as_deref()
        .unwrap_or(&config.email_from);
    send(
        config,
        &request.email,
        &subject,
        &html,
        &text,
        Some(reply_to),
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
    reply_to: Option<&str>,
) -> Result<(), EmailError> {
    match config.email_backend {
        EmailBackend::Console => {
            tracing::info!(to, subject, ?reply_to, "email (console backend)");
            // println! rather than tracing for the body: the pretty formatter
            // wraps long lines, which makes the link painful to copy.
            println!(
                "\n=== EMAIL to {to} ===\n{subject}\nReply-To: {}\n\n{text}\n=== END ===\n",
                reply_to.unwrap_or("(none)")
            );
            Ok(())
        }

        EmailBackend::Smtp => {
            let mut builder = Message::builder()
                .from(config.email_from.parse()?)
                .to(to.parse()?)
                .subject(subject);

            if let Some(reply_to) = reply_to {
                builder = builder.reply_to(reply_to.parse()?);
            }

            let message = builder
                // Both parts in one message: some clients show the text one,
                // and its absence hurts the spam score.
                .multipart(body_with_logo(text, html))?;

            smtp_transport(config)?.send(message).await?;
            Ok(())
        }

        EmailBackend::Resend => {
            let mut payload = serde_json::json!({
                "from": config.email_from,
                "to": to,
                "subject": subject,
                "html": html,
                "text": text,
                // Same logo, same `cid:` reference as the SMTP path, so the
                // two backends render the identical message.
                "attachments": [{
                    "filename": "email-icon.png",
                    "content_type": "image/png",
                    "content_id": LOGO_CID,
                    "content": base64::engine::general_purpose::STANDARD.encode(LOGO_BYTES),
                }],
            });

            if let Some(reply_to) = reply_to {
                payload["reply_to"] = serde_json::Value::String(reply_to.to_owned());
            }

            let response = reqwest::Client::new()
                .post("https://api.resend.com/emails")
                .bearer_auth(&config.resend_api_key)
                .json(&payload)
                .send()
                .await?;

            if !response.status().is_success() {
                return Err(EmailError::Rejected(
                    response.text().await.unwrap_or_default(),
                ));
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
        SmtpTls::StartTls => {
            AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&config.smtp_host)?
        }
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

    const PUBLIC_URL: &str = "https://ctf.example.test";

    /// A bare address and one with a display name make a mailbox; an empty
    /// atom, which the request validator lets through, does not.
    #[test]
    fn a_mailbox_is_parsed_as_the_smtp_path_parses_it() {
        for address in [
            "visitor@example.test",
            "PwnForge <noreply@ctf.example.test>",
        ] {
            assert!(is_valid_mailbox(address), "{address} is a mailbox");
        }
        for address in [
            "a..b@example.test",
            ".a@example.test",
            "a@[127.0.0.1]",
            "no-at-sign",
            "",
        ] {
            assert!(!is_valid_mailbox(address), "{address:?} is not a mailbox");
        }
    }

    /// A refusal by the provider is a delivery failure; a message that could
    /// not be built, or an address that does not parse, is not.
    #[test]
    fn only_the_mail_service_failing_counts_as_a_delivery_failure() {
        assert!(EmailError::Rejected("refused".to_owned()).is_delivery_failure());

        let unbuilt = EmailError::Message(lettre::error::Error::MissingFrom);
        assert!(!unbuilt.is_delivery_failure());
        let unparsed = EmailError::Address(
            "a..b@example.test"
                .parse::<lettre::Address>()
                .expect_err("an empty atom does not parse"),
        );
        assert!(!unparsed.is_delivery_failure());
    }

    fn password_reset_html(locale: &str, link: &str) -> PasswordResetHtml {
        let footer = Footer::new(locale);

        PasswordResetHtml {
            locale: locale.to_owned(),
            title: t!("email.password_reset.title", locale = locale).to_string(),
            preheader: t!("email.password_reset.preheader", locale = locale).to_string(),
            greeting: t!("email.common.greeting", locale = locale).to_string(),
            body: t!("email.password_reset.body", locale = locale).to_string(),
            cta: t!("email.password_reset.cta", locale = locale).to_string(),
            link: link.to_owned(),
            fallback_notice: t!("email.common.fallback_notice", locale = locale).to_string(),
            expiry: t!("email.password_reset.expiry", locale = locale).to_string(),
            ignore_notice: t!("email.common.ignore_notice", locale = locale).to_string(),
            footer: footer.text,
            footer_instance: footer.instance,
            footer_note: footer.note,
            public_url: PUBLIC_URL.to_owned(),
        }
    }

    #[test]
    fn password_reset_renders_in_both_languages() {
        let link = "https://example.test/auth/password/reset#token=abc123";

        for locale in ["en", "fr"] {
            let html = password_reset_html(locale, link)
                .render()
                .expect("template must render");

            assert!(html.contains(link), "link missing in {locale}");
            // Catches a variable that was never substituted.
            assert!(
                !html.contains("{{"),
                "unsubstituted placeholder in {locale}"
            );
            assert!(html.contains(&format!(r#"lang="{locale}""#)));
            // The instance URL is what tells the reader which deployment sent
            // this; someone self-hosting may well run several.
            assert!(
                html.contains(PUBLIC_URL),
                "instance url missing in {locale}"
            );
        }
    }

    #[test]
    fn password_reset_carries_a_hidden_preview_line() {
        let html = password_reset_html("en", "https://example.test/x")
            .render()
            .expect("template must render");

        let preheader = t!("email.password_reset.preheader", locale = "en").to_string();

        // Present in the markup and hidden from the body: an inbox preview that
        // just repeats the first sentence is the failure mode here.
        assert!(html.contains(&preheader));
        assert!(html.contains("mso-hide:all"));
    }

    #[test]
    fn password_reset_text_carries_the_link() {
        let footer = Footer::new("en");
        let link = "https://example.test/auth/password/reset#token=abc123";

        let text = PasswordResetText {
            title: t!("email.password_reset.title", locale = "en").to_string(),
            greeting: t!("email.common.greeting", locale = "en").to_string(),
            body: t!("email.password_reset.body", locale = "en").to_string(),
            cta: t!("email.password_reset.cta", locale = "en").to_string(),
            link: link.to_owned(),
            expiry: t!("email.password_reset.expiry", locale = "en").to_string(),
            ignore_notice: t!("email.common.ignore_notice", locale = "en").to_string(),
            footer: footer.text,
            footer_instance: footer.instance,
            footer_note: footer.note,
            public_url: PUBLIC_URL.to_owned(),
        }
        .render()
        .expect("template must render");

        assert!(text.contains(link));
    }

    #[test]
    fn security_alert_carries_no_action_link() {
        let footer = Footer::new("en");

        let html = SecurityAlertHtml {
            locale: "en".to_owned(),
            title: t!(AlertKind::PasswordChanged.title_key(), locale = "en").to_string(),
            preheader: t!(AlertKind::PasswordChanged.preheader_key(), locale = "en").to_string(),
            greeting: t!("email.common.greeting", locale = "en").to_string(),
            body: t!(AlertKind::PasswordChanged.body_key(), locale = "en").to_string(),
            label_when: t!("email.common.label_when", locale = "en").to_string(),
            label_ip: t!("email.common.label_ip", locale = "en").to_string(),
            label_device: t!("email.common.label_device", locale = "en").to_string(),
            occurred_at: "2026-08-19 14:03 UTC".to_owned(),
            ip_address: "203.0.113.7".to_owned(),
            user_agent: "Firefox on Fedora".to_owned(),
            no_action: t!("email.security_alert.no_action", locale = "en").to_string(),
            not_you: t!("email.security_alert.not_you", locale = "en").to_string(),
            footer: footer.text,
            footer_instance: footer.instance,
            footer_note: footer.note,
            public_url: PUBLIC_URL.to_owned(),
        }
        .render()
        .expect("template must render");

        assert!(html.contains("203.0.113.7"));
        assert!(!html.contains("{{"));

        // The only href a security alert may carry is the instance itself, in
        // the footer. An action link here would be the exact shape of a
        // phishing message; this assertion is what stops one being added.
        assert_eq!(html.matches("href=").count(), 1);
        assert!(html.contains(PUBLIC_URL));
    }

    /// `EMAIL_FROM` is handed to lettre as a single string and only becomes a
    /// display name if lettre parses the RFC 5322 `name-addr` form. This pins
    /// that behaviour, so a future lettre upgrade that dropped it would fail
    /// here rather than silently ship mail signed by a bare address.
    #[test]
    fn email_from_keeps_the_display_name() {
        let mailbox: lettre::message::Mailbox = "PwnForge <hello@pwnforge.app>"
            .parse()
            .expect("name-addr must parse");

        assert_eq!(mailbox.name.as_deref(), Some("PwnForge"));
        assert_eq!(mailbox.email.to_string(), "hello@pwnforge.app");

        // What actually lands in the `From:` header.
        assert_eq!(mailbox.to_string(), "PwnForge <hello@pwnforge.app>");

        // A name holding a comma has to come back out quoted, or it would read
        // as two recipients.
        let quoted: lettre::message::Mailbox = r#""PwnForge, CTF" <hello@pwnforge.app>"#
            .parse()
            .expect("quoted display name must parse");
        assert_eq!(
            quoted.to_string(),
            r#""PwnForge, CTF" <hello@pwnforge.app>"#
        );
    }

    /// The invitation mail renders for both kinds in both languages, carries
    /// its link and expiry, and leaves no placeholder behind. It takes no
    /// account at all, which is what keeps its form the same whether one
    /// holds the address or not.
    #[test]
    fn the_invitation_mail_renders_for_both_kinds_in_both_languages() {
        let expires_at = chrono::DateTime::parse_from_rfc3339("2026-10-21T09:30:00Z")
            .expect("valid date")
            .with_timezone(&chrono::Utc);

        for kind in [InvitationKind::Team, InvitationKind::Workspace] {
            for locale in ["en", "fr"] {
                let message = invitation_message(
                    PUBLIC_URL,
                    locale,
                    kind,
                    "Night Owls",
                    "tok123",
                    expires_at,
                )
                .expect("template must render");
                let link = format!("{PUBLIC_URL}/invitations/{}#token=tok123", kind.segment());

                for part in [&message.html, &message.text] {
                    assert!(part.contains(&link), "link missing in {locale}");
                    assert!(part.contains("Night Owls"), "name missing in {locale}");
                    assert!(
                        part.contains("2026-10-21 09:30"),
                        "expiry missing in {locale}"
                    );
                    assert!(
                        !part.contains("{{"),
                        "unsubstituted placeholder in {locale}"
                    );
                    assert!(!part.contains("%{"), "untranslated variable in {locale}");
                }
                assert!(!message.subject.is_empty());
                assert!(message.html.contains(&format!(r#"lang="{locale}""#)));
            }
        }
    }

    /// Every link that carries a token carries it in the fragment, which the
    /// browser never sends: a query string would land in the access log of
    /// every server and proxy the page load goes through.
    #[test]
    fn token_links_carry_the_token_in_the_fragment() {
        for (link, path) in [
            (reset_link(PUBLIC_URL, "tok123"), "/auth/password/reset"),
            (
                verification_link(PUBLIC_URL, "tok123"),
                "/auth/email/verify",
            ),
        ] {
            assert_eq!(link, format!("{PUBLIC_URL}{path}#token=tok123"));
            assert!(!link.contains('?'), "no query string in {link}");
        }
    }

    /// Fails on a key with no translation: `t!` hands the key itself back.
    fn assert_translated(rendered: &str, locale: &str) {
        assert!(
            !rendered.contains("email.signup_notice") && !rendered.contains("email.common"),
            "untranslated key in {locale}: {rendered}"
        );
    }

    /// The holder of a verified address is pointed at the sign-in and reset
    /// pages, by plain links, and handed no token.
    #[test]
    fn a_signup_notice_to_an_account_links_to_sign_in_and_reset_only() {
        for locale in ["en", "fr"] {
            let (subject, html, text) =
                render_signup_notice(PUBLIC_URL, locale, &SignupNotice::ExistingAccount)
                    .expect("the notice must render");

            assert_translated(&subject, locale);
            for part in [&html, &text] {
                assert_translated(part, locale);
                assert!(
                    part.contains(&format!("{PUBLIC_URL}/login")),
                    "sign-in link in {locale}"
                );
                assert!(
                    part.contains(&format!("{PUBLIC_URL}/forgot-password")),
                    "reset link in {locale}"
                );
                assert!(!part.contains("token"), "no token in {locale}");
            }
        }
    }

    /// A registration nobody confirmed is announced with the date it goes,
    /// and never with a link that could confirm it.
    #[test]
    fn a_signup_notice_to_a_pending_registration_gives_its_date_and_no_link() {
        let deleted_after = "2026-10-26T12:00:00Z"
            .parse::<chrono::DateTime<chrono::Utc>>()
            .expect("a valid date");

        for locale in ["en", "fr"] {
            let (subject, html, text) = render_signup_notice(
                PUBLIC_URL,
                locale,
                &SignupNotice::PendingRegistration { deleted_after },
            )
            .expect("the notice must render");

            assert_translated(&subject, locale);
            for part in [&html, &text] {
                assert_translated(part, locale);
                assert!(
                    part.contains("2026-10-26 12:00 UTC"),
                    "deletion date in {locale}"
                );
                assert!(!part.contains("token"), "no token in {locale}");
                assert!(
                    !part.contains("/auth/"),
                    "no link to an auth page in {locale}"
                );
                assert!(!part.contains("/login"), "no sign-in link in {locale}");
            }
        }
    }
}
