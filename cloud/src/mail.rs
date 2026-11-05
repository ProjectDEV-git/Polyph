//! Outgoing email: sign-in codes and team invites, in the recipient's
//! language. Texts live in `locales/*/cloud.ftl`.

use std::sync::Arc;

use parking_lot::Mutex;
use polyph_i18n::Localizer;
use tracing::info;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mail {
    pub to: String,
    pub subject: String,
    pub text: String,
}

#[derive(Debug, Clone)]
pub enum Mailer {
    /// Development: the message goes to the log.
    Log,
    /// A Resend-compatible HTTP endpoint.
    Http { endpoint: String, api_key: String, from: String, client: reqwest::Client },
    /// Tests: messages are kept in memory.
    Memory(Arc<Mutex<Vec<Mail>>>),
}

impl Mailer {
    pub fn from_config(cfg: &crate::config::CloudConfig) -> Self {
        match (&cfg.mail.endpoint, &cfg.mail.api_key) {
            (Some(e), Some(k)) => Mailer::Http {
                endpoint: e.clone(),
                api_key: k.clone(),
                from: cfg.mail.from.clone(),
                client: crate::http_client(),
            },
            _ => Mailer::Log,
        }
    }

    pub async fn send(&self, mail: Mail) -> anyhow::Result<()> {
        match self {
            Mailer::Log => {
                info!(to = %mail.to, subject = %mail.subject, text = %mail.text, "mail (dev mode, not sent)");
                Ok(())
            }
            Mailer::Memory(m) => {
                m.lock().push(mail);
                Ok(())
            }
            Mailer::Http { endpoint, api_key, from, client } => {
                let r = client
                    .post(endpoint)
                    .bearer_auth(api_key)
                    .json(&serde_json::json!({ "from": from, "to": [mail.to], "subject": mail.subject, "text": mail.text }))
                    .send()
                    .await?;
                if !r.status().is_success() {
                    anyhow::bail!("mail endpoint answered {}", r.status());
                }
                Ok(())
            }
        }
    }
}

pub fn code_mail(to: &str, lang: &str, code: &str) -> Mail {
    let l = Localizer::new(lang);
    Mail {
        to: to.to_string(),
        subject: l.t("cloud-mail-code-subject"),
        text: l.t_args("cloud-mail-code-body", &[("code", code.into())]),
    }
}

pub fn invite_mail(to: &str, lang: &str, team: &str, link: &str) -> Mail {
    let l = Localizer::new(lang);
    Mail {
        to: to.to_string(),
        subject: l.t_args("cloud-mail-invite-subject", &[("team", team.into())]),
        text: l.t_args("cloud-mail-invite-body", &[("team", team.into()), ("link", link.into())]),
    }
}
