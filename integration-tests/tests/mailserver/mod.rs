use crate::dkim_entry::DkimEntry;
use bollard::container::{AttachContainerOptions, AttachContainerResults, Config};
use bollard::exec::{CreateExecOptions, StartExecResults};
use bollard::image::CreateImageOptions;
use bollard::Docker;
use futures::{StreamExt, TryStreamExt};
use lettre::message::header::ContentType;
use lettre::message::Mailbox;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{Address, Message, SmtpTransport, Transport};
use std::collections::HashMap;
use tokio::io::AsyncWriteExt;
use tokio::spawn;

const IMAGE: &str = "mailserver/docker-mailserver:11";

pub struct Mailserver {
    docker: Docker,
    pub domain: String,
    pub ip_address: String,
    id: String,
    pub dkim_entry: DkimEntry,
}

impl Mailserver {
    async fn continuously_print_docker_output(docker: &Docker, id: &str) -> anyhow::Result<()> {
        let AttachContainerResults { mut output, .. } = docker
            .attach_container(
                &id,
                Some(AttachContainerOptions::<String> {
                    stdout: Some(true),
                    stderr: Some(true),
                    stream: Some(true),
                    ..Default::default()
                }),
            )
            .await?;

        // Asynchronous process that pipes docker attach output into stdout.
        // Will die automatically once Docker container output is closed.
        spawn(async move {
            let mut stdout = tokio::io::stdout();

            while let Some(Ok(output)) = output.next().await {
                stdout
                    .write_all(output.into_bytes().as_ref())
                    .await
                    .unwrap();
                stdout.flush().await.unwrap();
            }
        });

        Ok(())
    }

    async fn generate_dkim(docker: &Docker, id: &str, domain: &str) -> anyhow::Result<String> {
        let exec = docker
            .create_exec(
                id,
                CreateExecOptions {
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    cmd: Some(vec!["setup", "config", "dkim", "domain", domain]),
                    ..Default::default()
                },
            )
            .await?
            .id;
        if let StartExecResults::Attached { mut output, .. } =
            docker.start_exec(&exec, None).await?
        {
            while let Some(Ok(msg)) = output.next().await {
                print!("{}", msg);
            }
        } else {
            anyhow::bail!("Failed to generate DKIM entry");
        }

        let exec = docker
            .create_exec(
                id,
                CreateExecOptions {
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    cmd: Some(vec![
                        "cat",
                        &format!("/tmp/docker-mailserver/opendkim/keys/{}/mail.txt", domain),
                    ]),
                    ..Default::default()
                },
            )
            .await?
            .id;
        let mut dkim_entry = String::new();
        if let StartExecResults::Attached { mut output, .. } =
            docker.start_exec(&exec, None).await?
        {
            while let Some(Ok(msg)) = output.next().await {
                dkim_entry.push_str(&msg.to_string());
            }
        } else {
            anyhow::bail!("Failed to fetch DKIM entry");
        }

        // Restart container to active DKIM signatures. See https://docker-mailserver.github.io/docker-mailserver/edge/config/best-practices/dkim/#enabling-dkim-signature.
        docker.restart_container(&id, None).await?;

        Ok(dkim_entry)
    }

    pub async fn new(domain: &str) -> anyhow::Result<Self> {
        let docker = Docker::connect_with_local_defaults()?;

        docker
            .create_image(
                Some(CreateImageOptions {
                    from_image: IMAGE,
                    ..Default::default()
                }),
                None,
                None,
            )
            .try_collect::<Vec<_>>()
            .await?;

        let empty = HashMap::<(), ()>::new();
        let mut exposed_ports = HashMap::new();
        exposed_ports.insert("25/tcp", empty.clone()); // SMTP
        exposed_ports.insert("143/tcp", empty); // IMAP4

        let mailserver_config = Config {
            image: Some(IMAGE),
            tty: Some(true),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            domainname: Some(domain),
            exposed_ports: Some(exposed_ports),
            ..Default::default()
        };

        let id = docker
            .create_container::<&str, &str>(None, mailserver_config)
            .await?
            .id;
        docker.start_container::<String>(&id, None).await?;

        let ip_address = docker
            .inspect_container(&id, None)
            .await?
            .network_settings
            .unwrap()
            .ip_address
            .unwrap();

        // Mailserver requires at least one email to be set up in order to start working
        Self::create_email_address(&docker, &id, &domain, "admin", "12345").await?;

        let dkim_entry = Self::generate_dkim(&docker, &id, &domain).await?;
        let mut dkim_entry = crate::dkim_entry::parse(&dkim_entry)?;
        dkim_entry.key = "mail._domainkey.email.near.org".to_string();

        Self::continuously_print_docker_output(&docker, &id).await?;

        let server = Self {
            docker,
            domain: domain.to_string(),
            ip_address,
            id,
            dkim_entry,
        };

        Ok(server)
    }

    async fn create_email_address(
        docker: &Docker,
        id: &str,
        domain: &str,
        username: &str,
        password: &str,
    ) -> anyhow::Result<(Address, Credentials)> {
        let address = Address::new(username, domain)
            .map_err(|e| anyhow::anyhow!("Could not construct a valid address: {}", e))?;
        let creds = Credentials::new(address.to_string(), password.to_string());
        let exec = docker
            .create_exec(
                id,
                CreateExecOptions {
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    cmd: Some(vec![
                        "setup",
                        "email",
                        "add",
                        &address.to_string(),
                        password,
                    ]),
                    ..Default::default()
                },
            )
            .await?
            .id;
        if let StartExecResults::Attached { mut output, .. } =
            docker.start_exec(&exec, None).await?
        {
            while let Some(Ok(msg)) = output.next().await {
                print!("{}", msg);
            }
        } else {
            anyhow::bail!("Failed to create {}", &address);
        }

        Ok((address, creds))
    }

    pub async fn create_email(
        &self,
        username: &str,
        password: &str,
    ) -> anyhow::Result<(Address, Credentials)> {
        Self::create_email_address(&self.docker, &self.id, &self.domain, username, password).await
    }

    pub fn init(
        &self,
        from: &Address,
        to: &Address,
        from_creds: &Credentials,
    ) -> anyhow::Result<()> {
        let email = Message::builder()
            .from(Mailbox::new(None, from.clone()))
            .to(Mailbox::new(None, to.clone()))
            .subject("init")
            .header(ContentType::TEXT_PLAIN)
            .body("".to_string())?;

        // Open a remote connection to gmail
        let mailer = SmtpTransport::builder_dangerous(&self.ip_address)
            .credentials(from_creds.clone())
            .build();

        // Send the email
        mailer.send(&email)?;

        Ok(())
    }
}
