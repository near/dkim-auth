use std::time::Duration;

use serde_json::json;
use test_log::test;
use tokio::spawn;
use workspaces::result::Execution;

mod dkim_entry;
mod mailserver;

#[test(tokio::test)]
async fn test_bollard() -> anyhow::Result<()> {
    let dkim_controller_wasm = workspaces::compile_project("../dkim-controller").await?;
    let mailserver = mailserver::Mailserver::new("email.near.org").await?;

    let (auth_address, _) = mailserver.create_email("authservice", "12345").await?;
    let (user_address, user_creds) = mailserver.create_email("user", "67890").await?;

    let worker = workspaces::testnet().await?;
    let (account_id, secret_key) = worker.dev_generate().await;
    let Execution {
        result: contract, ..
    } = worker
        .create_tla_and_deploy(
            account_id.clone(),
            secret_key.clone(),
            &dkim_controller_wasm,
        )
        .await?;

    contract
        .call("new")
        .args_json(json!({
            "dkim_entries": [[&mailserver.dkim_entry.key, &mailserver.dkim_entry.value]]
        }))
        .transact()
        .await?
        .into_result()?;

    let ip_address = mailserver.ip_address.clone();
    let email_relayer_address = auth_address.clone();
    let handle = spawn(async move {
        email_relayer::run(
            &account_id,
            &secret_key,
            &account_id,
            &ip_address,
            false,
            &email_relayer_address,
            &"12345",
        )
        .await
        .unwrap();
    });

    tokio::time::sleep(Duration::from_secs(1)).await;

    mailserver.init(&user_address, &auth_address, &user_creds)?;

    handle.await?;

    Ok(())
}
