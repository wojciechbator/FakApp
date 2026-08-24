//! SMTP delivery through lettre. One transport, built once; alerts are rare
//! by design and must not pay connection setup more than necessary.

use anyhow::Context;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

use crate::config::Smtp;

pub struct Mailer {
    transport: AsyncSmtpTransport<Tokio1Executor>,
    from: String,
    to: Vec<String>,
}

impl Mailer {
    pub fn new(smtp: &Smtp) -> anyhow::Result<Mailer> {
        let relay: AsyncSmtpTransport<Tokio1Executor> =
            AsyncSmtpTransport::<Tokio1Executor>::relay(&smtp.host)?
                .port(smtp.port)
                .credentials(lettre::transport::smtp::authentication::Credentials::new(
                    smtp.user.clone(),
                    smtp.password.clone(),
                ))
                .build();
        Ok(Mailer {
            transport: relay,
            from: smtp.from.clone(),
            to: smtp.to.clone(),
        })
    }

    /// Sends one message to every recipient. The first hard failure stops the
    /// loop and is reported to the caller (the checker logs it and waits for
    /// the reminder window instead of pretending the page went out).
    pub async fn send(&self, subject: &str, body: &str) -> anyhow::Result<()> {
        for recipient in &self.to {
            let email = Message::builder()
                .from(
                    self.from
                        .parse()
                        .with_context(|| format!("invalid From address {:?}", self.from))?,
                )
                .to(recipient
                    .parse()
                    .with_context(|| format!("invalid To address {recipient:?}"))?)
                .subject(subject)
                .body(body.to_owned())
                .context("building alert mail")?;
            self.transport
                .send(email)
                .await
                .map_err(|error| anyhow::anyhow!("smtp send to {recipient} failed: {error}"))?;
        }
        Ok(())
    }
}
