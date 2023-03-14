use imap::Session;
use native_tls::TlsStream;
use serde_json::json;
use std::str::FromStr;
use std::{env, net::TcpStream, time::Duration};
use workspaces::{types::SecretKey, Network};
use workspaces::{AccountId, InMemorySigner, Worker};

fn create_imap_session(
    username: &str,
    password: &str,
) -> anyhow::Result<Session<TlsStream<TcpStream>>> {
    let domain = "imap.gmail.com";
    let tls = native_tls::TlsConnector::builder().build()?;

    // we pass in the domain twice to check that the server's TLS
    // certificate is valid for the domain we're connecting to.
    let client = imap::connect((domain, 993), domain, &tls)?;

    // the client we have here is unauthenticated.
    // to do anything useful with the e-mails, we need to log in
    Ok(client.login(username, password).map_err(|e| e.0)?)
}

fn fetch_inbox_from(
    min_value: u32,
    username: &str,
    password: &str,
) -> anyhow::Result<(u32, Vec<String>)> {
    // Ensure that sessions are short-lived to avoid timeouts
    let mut imap_session = create_imap_session(username, password)?;

    // Opens mailbox in read-only mode.
    // TODO: Make sure that INBOX is the correct name for our email provider of choosing.
    let mailbox = imap_session.examine("INBOX")?;

    let result = if mailbox.exists > min_value {
        let max_value = mailbox.exists;
        let mail_indices = (min_value + 1..=max_value).map(|x| x.to_string());

        let messages = imap_session.fetch(itertools::join(mail_indices, ","), "RFC822")?;

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

fn parse_env_var<T: FromStr>(key: &str) -> anyhow::Result<T> {
    Ok(env::var(key)
        .map_err(|err| anyhow::anyhow!("Failed to get {}: {}", key, err))?
        .parse()
        .map_err(|_| anyhow::anyhow!("Failed to parse {}", key))?)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let signer_account: AccountId = parse_env_var("SENDER_SIGNER_NEAR_ACCOUNT")?;
    let signer_account_secret_key: SecretKey = parse_env_var("SENDER_SIGNER_SECRET_KEY")?;
    let controller_account: AccountId = parse_env_var("SENDER_CONTROLLER_NEAR_ACCOUNT")?;
    let imap_username: String = parse_env_var("SENDER_IMAP_USERNAME")?;
    let imap_password: String = parse_env_var("SENDER_IMAP_PASSWORD")?;

    // TODO: Make network dynamically choosable based on SENDER_NEAR_NETWORK
    let worker = workspaces::testnet().await?;
    let signer = InMemorySigner::from_secret_key(signer_account, signer_account_secret_key);

    println!("Starting...");

    // TODO: Properly detect unprocessed emails. See https://github.com/near/email-auth/issues/4
    let mut min_value = 0;
    let (value, _) = fetch_inbox_from(min_value, &imap_username, &imap_password)?;
    min_value = value;
    println!("Already {} email present - ignoring.", min_value);

    loop {
        let (value, mails) = fetch_inbox_from(min_value, &imap_username, &imap_password)?;
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
