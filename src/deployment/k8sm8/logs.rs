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
) -> Result<impl AsyncBufRead> {
    let api: Api<Pod> = Api::namespaced(client, namespace);
    let log_params = LogParams {
        follow: true,
        timestamps: true,
        since_seconds: Some(120), // Stream logs for the last 60 seconds
        ..Default::default()
    };

    let stream = api.log_stream(&name, &log_params).await?;

    Ok(stream)
}

pub async fn get_logs(
    client: Client,
    namespace: &str,
    pod_name: &str,
) -> Result<impl AsyncBufRead> {
    let api: Api<Pod> = Api::namespaced(client, namespace);
    let log_params = LogParams {
        follow: false,
        timestamps: true,
        ..Default::default()
    };

    let stream = api.log_stream(pod_name, &log_params).await?;

    Ok(stream)
}

pub async fn log_merger(client: Client, namespace: &str, selected_pods: Vec<String>) -> Result<()> {
    if selected_pods.is_empty() {
        println!("No pods selected.");
        return Ok(());
    }

    // Create a future for each pod's log stream.
    let streams: Vec<_> = stream::iter(selected_pods)
        .map(|pod_name| {
            let client = client.clone();
            async move {
                let stream = get_logs(client, namespace, &pod_name).await?;
                let lines = stream.lines();
                // `log_tagger` now returns a pinned, boxed stream.
                let tagged_stream = log_tagger(lines, pod_name.to_string());
                Ok(tagged_stream)
            }
        })
        .buffer_unordered(16) // Run up to 16 stream creations concurrently.
        .filter_map(|res: Result<_>| async { res.ok() }) // Keep only successfully created streams.
        .collect()
        .await;

    println!(
        "Merging logs from {} pods... Press Ctrl+C to exit.",
        streams.len()
    );

    // `select_all` can now work with our Vec of `Unpin` streams.
    let mut merged_stream = stream::select_all(streams);
    let mut grouper = log_grouper();
    // The stream yields `(String, String)`. The pattern `(pod_name, log_line)` moves these

    // owned Strings into the variables. The compiler errors about `str` were because it
    // couldn't correctly infer the stream's item type due to the upstream `Unpin` error.
    // This loop will run until the stream is exhausted or an error occurs.
    while let Some((pod_name, log_line)) = merged_stream.next().await {
        grouper.transform_and_print(&pod_name, &log_line);
    }

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

    let namespace = namespace.to_string(); // Clone the namespace to move into the task.

    // The `processing_task` now takes ownership of the `namespace` String.
    let processing_task = tokio::spawn(async move {
        let streams: Vec<_> = stream::iter(selected_pods)
            .map(|pod_name| {
                let client = client.clone();
                // Clone the namespace String for each stream setup.
                // Cloning a String is cheap and necessary for ownership.
                let namespace_clone = namespace.clone();
                async move {
                    // Pass a reference (`&`) to the owned String.
                    let stream = get_stream(client, &namespace_clone, pod_name.clone()).await?;
                    let lines = stream.lines();
                    Ok(log_tagger(lines, pod_name.to_string()))
                }
            })
            .buffer_unordered(10)
            .filter_map(|res: Result<_>| async { res.ok() })
            .collect()
            .await;

        let mut merged_stream = stream::select_all(streams);
        println!(
            "Streaming logs from {} pods... Press Ctrl+C to exit.",
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
