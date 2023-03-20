mod dkim_entry;
mod mailserver;

use serde_json::json;
use test_log::test;

#[test(tokio::test)]
async fn test_bollard() -> anyhow::Result<()> {
    let dkim_controller_wasm = workspaces::compile_project("../dkim-controller").await?;
    let mailserver = mailserver::Mailserver::new().await?;

    println!(
        "DKIM: {} {}",
        &mailserver.dkim_entry.key, &mailserver.dkim_entry.value
    );

    let username = "daniyar";
    let password = "daniyar123";

    let daniyar_email = mailserver.create_email(username, password).await?;

    let worker = workspaces::testnet().await?;
    let (account_id, secret_key) = worker.dev_generate().await;
    let contract = worker
        .create_tla_and_deploy(
            account_id.clone(),
            secret_key.clone(),
            &dkim_controller_wasm,
        )
        .await?
        .result;

    contract
        .call("new")
        .args_json(json!({
            "dkim_entries": [[&mailserver.dkim_entry.key, &mailserver.dkim_entry.key]]
        }))
        .transact()
        .await?
        .into_result()?;

    email_relayer::run(
        &account_id,
        &secret_key,
        &account_id,
        &mailserver.ip_address,
        false,
        &daniyar_email,
        password,
    )
    .await?;

    Ok(())
}
