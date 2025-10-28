use anyhow::Result;
use futures::{stream, AsyncBufRead, AsyncBufReadExt, StreamExt};
use k8s_openapi::api::core::v1::Pod;
use kube::{api::LogParams, Api, Client};
use tokio::sync::mpsc;

use super::processing::{log_grouper, log_tagger, TaggedLog};

struct ProcessedLog {
    pod_name: String,
    content: String,
}

fn process_log_line(tagged_log: TaggedLog) -> ProcessedLog {
    let (pod_name, log_line) = tagged_log;
    let content = format!("{}", log_line);

    ProcessedLog { pod_name, content }
}

pub async fn get_stream(
    client: Client,
    namespace: &str,
    name: String,
    container_name: Option<String>,
) -> Result<impl AsyncBufRead> {
    let api: Api<Pod> = Api::namespaced(client, namespace);
    let log_params = LogParams {
        follow: true,
        timestamps: true,
        since_seconds: Some(120),
        container: container_name,
        ..Default::default()
    };

    let stream = api.log_stream(&name, &log_params).await?;

    Ok(stream)
}

pub async fn get_logs(
    client: Client,
    namespace: &str,
    pod_name: String,
    container_name: Option<String>,
) -> Result<impl AsyncBufRead> {
    let api: Api<Pod> = Api::namespaced(client, namespace);
    let log_params = LogParams {
        follow: false,
        timestamps: true,
        container: container_name,
        ..Default::default()
    };

    let stream = api.log_stream(&pod_name, &log_params).await?;

    Ok(stream)
}

pub async fn log_merger(client: Client, namespace: &str, selected_pods: Vec<String>) -> Result<()> {
    if selected_pods.is_empty() {
        println!("No pods selected.");
        return Ok(());
    }

    let (tx, mut rx) = mpsc::channel::<ProcessedLog>(100);

    let printer_task = tokio::spawn(async move {
        let mut grouper = log_grouper();
        while let Some(processed_log) = rx.recv().await {
            grouper.transform_and_print(&processed_log.pod_name, &processed_log.content);
        }
    });

    let namespace = namespace.to_string();

    let pod_and_containers: Vec<(String, String)> = stream::iter(selected_pods)
        .then(|pod_name| {
            let client = client.clone();
            let namespace = namespace.clone();
            async move {
                let api: Api<Pod> = Api::namespaced(client, &namespace);
                let containers = match api.get(&pod_name).await {
                    Ok(pod) => pod.spec.map(|s| s.containers).unwrap_or_default(),
                    Err(e) => {
                        eprintln!("Error getting pod {}: {}", pod_name, e);
                        vec![]
                    }
                };
                let pod_and_containers = containers
                    .into_iter()
                    .map(move |c| (pod_name.clone(), c.name));
                futures::stream::iter(pod_and_containers)
            }
        })
        .flatten()
        .collect()
        .await;

    let processing_task = tokio::spawn(async move {
        let streams: Vec<_> = stream::iter(pod_and_containers)
            .map(|(pod_name, container_name)| {
                let client = client.clone();
                let namespace_clone = namespace.clone();
                async move {
                    let stream = get_logs(
                        client,
                        &namespace_clone,
                        pod_name.clone(),
                        Some(container_name.clone()),
                    )
                    .await?;
                    let lines = stream.lines();
                    let tag = format!("{}/{}", pod_name, container_name);
                    Ok(log_tagger(lines, tag))
                }
            })
            .buffer_unordered(20)
            .filter_map(|res: Result<_>| async { res.ok() })
            .collect()
            .await;

        let mut merged_stream = stream::select_all(streams);
        println!(
            "Merging logs from {} containers... Press Ctrl+C to exit.",
            merged_stream.len()
        );

        while let Some(tagged_log) = merged_stream.next().await {
            let tx_clone = tx.clone();
            tokio::spawn(async move {
                let processed = process_log_line(tagged_log);
                if let Err(e) = tx_clone.send(processed).await {
                    eprintln!("Failed to send processed log to printer: {}", e);
                }
            });
        }
    });

    let _ = tokio::try_join!(printer_task, processing_task);

    Ok(())
}

pub async fn log_streamer(
    client: Client,
    namespace: &str,
    selected_pods: Vec<String>,
) -> Result<()> {
    if selected_pods.is_empty() {
        println!("No pods selected.");
        return Ok(());
    }

    let (tx, mut rx) = mpsc::channel::<ProcessedLog>(100);

    let printer_task = tokio::spawn(async move {
        let mut grouper = log_grouper();
        while let Some(processed_log) = rx.recv().await {
            grouper.transform_and_print(&processed_log.pod_name, &processed_log.content);
        }
    });

    let namespace = namespace.to_string();

    let pod_and_containers: Vec<(String, String)> = stream::iter(selected_pods)
        .then(|pod_name| {
            let client = client.clone();
            let namespace = namespace.clone();
            async move {
                let api: Api<Pod> = Api::namespaced(client, &namespace);
                let containers = match api.get(&pod_name).await {
                    Ok(pod) => pod.spec.map(|s| s.containers).unwrap_or_default(),
                    Err(e) => {
                        eprintln!("Error getting pod {}: {}", pod_name, e);
                        vec![]
                    }
                };
                let pod_and_containers = containers
                    .into_iter()
                    .map(move |c| (pod_name.clone(), c.name));
                futures::stream::iter(pod_and_containers)
            }
        })
        .flatten()
        .collect()
        .await;

    let processing_task = tokio::spawn(async move {
        let streams: Vec<_> = stream::iter(pod_and_containers)
            .map(|(pod_name, container_name)| {
                let client = client.clone();
                let namespace_clone = namespace.clone();
                async move {
                    let stream = get_stream(
                        client,
                        &namespace_clone,
                        pod_name.clone(),
                        Some(container_name.clone()),
                    )
                    .await?;
                    let lines = stream.lines();
                    let tag = format!("{}/{}", pod_name, container_name);
                    Ok(log_tagger(lines, tag))
                }
            })
            .buffer_unordered(20)
            .filter_map(|res: Result<_>| async { res.ok() })
            .collect()
            .await;

        let mut merged_stream = stream::select_all(streams);
        println!(
            "Streaming logs from {} containers... Press Ctrl+C to exit.",
            merged_stream.len()
        );

        while let Some(tagged_log) = merged_stream.next().await {
            let tx_clone = tx.clone();
            tokio::spawn(async move {
                let processed = process_log_line(tagged_log);
                if let Err(e) = tx_clone.send(processed).await {
                    eprintln!("Failed to send processed log to printer: {}", e);
                }
            });
        }
    });

    let _ = tokio::try_join!(printer_task, processing_task);
    Ok(())
}
