use anyhow::Result;
use futures::{stream, AsyncBufRead, AsyncBufReadExt, StreamExt};
use k8s_openapi::api::core::v1::Pod;
use kube::{api::LogParams, Api, Client};

use super::processing::{log_grouper, log_tagger};

pub async fn get_stream(client: Client, namespace: &str, name: &str) -> Result<impl AsyncBufRead> {
    let api: Api<Pod> = Api::namespaced(client, namespace);
    let log_params = LogParams {
        follow: true,
        timestamps: true,
        since_seconds: Some(120), // Stream logs for the last 60 seconds
        ..Default::default()
    };

    let stream = api.log_stream(name, &log_params).await?;

    Ok(stream)
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

    // Create a future for each pod's tagged log stream.
    // The type of `streams` will now be `Vec<Pin<Box<dyn Stream<Item = TaggedLog> + Send>>>`.
    let streams: Vec<_> = stream::iter(selected_pods)
        .map(|pod_name| {
            let client = client.clone();
            async move {
                let stream = get_stream(client, namespace, &pod_name).await?;
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
        "Streaming logs from {} pods... Press Ctrl+C to exit.",
        streams.len()
    );

    // `select_all` can now work with our Vec of `Unpin` streams.
    let mut merged_stream = stream::select_all(streams);

    let mut grouper = log_grouper();

    // The stream yields `(String, String)`. The pattern `(pod_name, log_line)` moves these
    // owned Strings into the variables. The compiler errors about `str` were because it
    // couldn't correctly infer the stream's item type due to the upstream `Unpin` error.
    while let Some((pod_name, log_line)) = merged_stream.next().await {
        grouper.transform_and_print(&pod_name, &log_line);
    }

    Ok(())
}
