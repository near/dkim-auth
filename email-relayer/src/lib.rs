use imap::types::{Fetches, Mailbox};
use imap::Session;
use native_tls::TlsStream;
use serde_json::json;
use std::{net::TcpStream, time::Duration};
use workspaces::{types::SecretKey, Network};
use workspaces::{AccountId, InMemorySigner, Worker};

trait Imap {
    fn examine(&mut self, mailbox_name: &str) -> imap::error::Result<Mailbox>;
    fn fetch(&mut self, sequence_set: &str, query: &str) -> imap::error::Result<Fetches>;
    fn logout(&mut self) -> imap::error::Result<()>;
}

impl Imap for Session<TcpStream> {
    fn examine(&mut self, mailbox_name: &str) -> imap::error::Result<Mailbox> {
        self.examine(mailbox_name)
    }

    fn fetch(&mut self, sequence_set: &str, query: &str) -> imap::error::Result<Fetches> {
        self.fetch(sequence_set, query)
    }

    fn logout(&mut self) -> imap::error::Result<()> {
        self.logout()
    }
}

impl Imap for Session<TlsStream<TcpStream>> {
    fn examine(&mut self, mailbox_name: &str) -> imap::error::Result<Mailbox> {
        self.examine(mailbox_name)
    }

    fn fetch(&mut self, sequence_set: &str, query: &str) -> imap::error::Result<Fetches> {
        self.fetch(sequence_set, query)
    }

    fn logout(&mut self) -> imap::error::Result<()> {
        self.logout()
    }
}

fn create_imap_session(
    domain: &str,
    ssl: bool,
    username: &str,
    password: &str,
) -> anyhow::Result<Box<dyn Imap>> {
    if ssl {
        let client = imap::ClientBuilder::new(domain, 993).native_tls()?;
        Ok(Box::new(client.login(username, password).map_err(|e| e.0)?))
    } else {
        let stream = TcpStream::connect(format!("{}:143", domain)).unwrap();
        let client = imap::Client::new(stream);
        Ok(Box::new(client.login(username, password).map_err(|e| e.0)?))
    }
}

fn fetch_inbox_from(
    min_value: u32,
    domain: &str,
    ssl: bool,
    username: &str,
    password: &str,
) -> anyhow::Result<(u32, Vec<String>)> {
    // Ensure that sessions are short-lived to avoid timeouts
    let mut imap_session = create_imap_session(domain, ssl, username, password)?;

    // Opens mailbox in read-only mode.
    // TODO: Make sure that INBOX is the correct name for our email provider of choosing.
    let mailbox = imap_session.examine("INBOX")?;

    let result = if mailbox.exists > min_value {
        let max_value = mailbox.exists;
        let mail_indices = (min_value + 1..=max_value).map(|x| x.to_string());

        let messages = imap_session.fetch(&itertools::join(mail_indices, ","), "RFC822")?;

        Ok((
            max_value,
            messages
                .iter()
                .map(|message| {
                    let body = message.body().expect("message did not have a body!");
                    std::str::from_utf8(body)
                        .expect("message was not valid utf-8")
                        .to_string()
                })
                .collect(),
        ))
    } else {
        Ok((min_value, vec![]))
    };
    imap_session.logout()?;

    result
}

async fn send_mail<N: Network + ?Sized>(
    worker: &Worker<N>,
    signer: &InMemorySigner,
    controller_account_id: &AccountId,
    mail: &str,
) -> anyhow::Result<()> {
    let payload = mail.as_bytes().to_vec();
    worker
        .call(signer, controller_account_id, "receive_email")
        .args(json!({ "full_email": payload }).to_string().into_bytes())
        .max_gas()
        .transact()
        .await?
        .into_result()?;
    Ok(())
}

pub async fn run(
    signer_account: &AccountId,
    signer_account_secret_key: &SecretKey,
    controller_account: &AccountId,
    domain: &str,
    ssl: bool,
    imap_username: &str,
    imap_password: &str,
) -> anyhow::Result<()> {
    // TODO: Make network dynamically choosable based on SENDER_NEAR_NETWORK
    let worker = workspaces::testnet().await?;
    let signer =
        InMemorySigner::from_secret_key(signer_account.clone(), signer_account_secret_key.clone());

    println!("Starting...");

    // TODO: Properly detect unprocessed emails. See https://github.com/near/email-auth/issues/4
    let mut min_value = 0;
    let (value, _) = fetch_inbox_from(min_value, domain, ssl, imap_username, imap_password)?;
    min_value = value;
    println!("Already {} email present - ignoring.", min_value);

    loop {
        let (value, mails) =
            fetch_inbox_from(min_value, domain, ssl, imap_username, imap_password)?;
        min_value = value;
        if !mails.is_empty() {
            println!("Got new mail: {:?}", mails.len());
            for mail in mails.iter() {
                send_mail(&worker, &signer, &controller_account, mail).await?;
            }
        }
        tokio::time::sleep(Duration::from_secs(10)).await;
    }
}
