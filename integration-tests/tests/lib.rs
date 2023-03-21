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
    let (user1_address, user1_creds) = mailserver.create_email("user1", "67890").await?;
    let (user2_address, user2_creds) = mailserver.create_email("user2", "09876").await?;

    let worker = workspaces::testnet().await?;
    let (account_id, secret_key) = worker.dev_generate().await;
    let Execution {
        result: contract,
        details,
        ..
    } = worker
        .create_tla_and_deploy(
            account_id.clone(),
            secret_key.clone(),
            &dkim_controller_wasm,
        )
        .await?;

    println!("Account {} was created by {:?}", account_id, details);

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

    mailserver.init(&user1_address, &auth_address, &user1_creds)?;
    mailserver.init(&user2_address, &auth_address, &user2_creds)?;

    mailserver.transfer(
        &user1_address,
        &user2_address,
        1.0,
        &auth_address,
        &user1_creds,
    )?;

    handle.await?;

    Ok(())
}
