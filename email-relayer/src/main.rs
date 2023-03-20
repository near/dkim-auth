use std::env;
use std::str::FromStr;
use workspaces::types::SecretKey;
use workspaces::AccountId;

fn parse_env_var<T: FromStr>(key: &str) -> anyhow::Result<T> {
    Ok(env::var(key)
        .map_err(|err| anyhow::anyhow!("Failed to get {}: {}", key, err))?
        .parse()
        .map_err(|_| anyhow::anyhow!("Failed to parse {}", key))?)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let signer_account: AccountId = parse_env_var("SENDER_SIGNER_NEAR_ACCOUNT")?;
    let signer_account_secret_key: SecretKey = parse_env_var("SENDER_SIGNER_SECRET_KEY")?;
    let controller_account: AccountId = parse_env_var("SENDER_CONTROLLER_NEAR_ACCOUNT")?;
    let imap_username: String = parse_env_var("SENDER_IMAP_USERNAME")?;
    let imap_password: String = parse_env_var("SENDER_IMAP_PASSWORD")?;

    email_relayer::run(
        &signer_account,
        &signer_account_secret_key,
        &controller_account,
        "imap.gmail.com",
        true,
        &imap_username,
        &imap_password,
    )
    .await
}
