use crate::dkim_entry::DkimEntry;
use bollard::container::{AttachContainerOptions, AttachContainerResults, Config};
use bollard::exec::{CreateExecOptions, StartExecResults};
use bollard::image::CreateImageOptions;
use bollard::Docker;
use futures::{StreamExt, TryStreamExt};
use std::collections::HashMap;
use tokio::io::AsyncWriteExt;
use tokio::spawn;

const IMAGE: &str = "mailserver/docker-mailserver:11";

pub struct Mailserver {
    docker: Docker,
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

    async fn generate_dkim(docker: &Docker, id: &str, ip_address: &str) -> anyhow::Result<String> {
        let exec = docker
            .create_exec(
                id,
                CreateExecOptions {
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    cmd: Some(vec!["setup", "config", "dkim", "domain", ip_address]),
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
                        &format!(
                            "/tmp/docker-mailserver/opendkim/keys/{}/mail.txt",
                            ip_address
                        ),
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

        Ok(dkim_entry)
    }

    pub async fn new() -> anyhow::Result<Self> {
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
        exposed_ports.insert("143/tcp", empty.clone());
        exposed_ports.insert("993/tcp", empty);

        let mailserver_config = Config {
            image: Some(IMAGE),
            tty: Some(true),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            hostname: Some("mail"),
            domainname: Some("near.org"),
            exposed_ports: Some(exposed_ports),
            ..Default::default()
        };

        let id = docker
            .create_container::<&str, &str>(None, mailserver_config)
            .await?
            .id;
        docker.start_container::<String>(&id, None).await?;

        Self::continuously_print_docker_output(&docker, &id).await?;

        let ip_address = docker
            .inspect_container(&id, None)
            .await?
            .network_settings
            .unwrap()
            .ip_address
            .unwrap();

        let dkim_entry = Self::generate_dkim(&docker, &id, &ip_address).await?;
        let dkim_entry = crate::dkim_entry::parse(&dkim_entry)?;

        let server = Self {
            docker,
            ip_address,
            id,
            dkim_entry,
        };

        // Mailserver requires at least one email to be set up in order to start working
        server.create_email("admin", "12345").await?;

        Ok(server)
    }

    pub async fn create_email(&self, username: &str, password: &str) -> anyhow::Result<String> {
        let email = format!("{}@{}", username, self.ip_address);
        let exec = self
            .docker
            .create_exec(
                &self.id,
                CreateExecOptions {
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    cmd: Some(vec!["setup", "email", "add", &email, password]),
                    ..Default::default()
                },
            )
            .await?
            .id;
        if let StartExecResults::Attached { mut output, .. } =
            self.docker.start_exec(&exec, None).await?
        {
            while let Some(Ok(msg)) = output.next().await {
                print!("{}", msg);
            }
        } else {
            anyhow::bail!("Failed to create {}", email);
        }

        Ok(email)
    }
}
