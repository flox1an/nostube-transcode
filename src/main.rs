mod r#const;
mod env;
mod helpers;

use crate::helpers::blossom::BlossomClient;
use crate::helpers::dvm::{get_input_url, get_relay_hints};
use crate::helpers::ffmpeg::{make_content_addressable, process_video, TransformConfig};
use crate::r#const::{DVM_STATUS_KIND, DVM_VIDEO_TRANSFORM_REQUEST_KIND, DVM_VIDEO_TRANSFORM_RESULT_KIND};
use nostr_sdk::prelude::*;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time;

struct AppState {
    seen_events: Arc<Mutex<HashSet<EventId>>>,
    client: Client,
    blossom_client: BlossomClient,
    config: env::Config,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logger
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();

    log::info!("Starting nostube-transcode DVM");

    // Load configuration
    let config = env::Config::from_env()?;
    log::info!("Loaded configuration");
    log::info!("Relays: {:?}", config.nostr_relays);
    log::info!("Blossom servers: {:?}", config.blossom_upload_servers);

    // Initialize Nostr keys
    let keys = Keys::parse(&config.nostr_private_key)?;
    log::info!("DVM Public Key: {}", keys.public_key().to_hex());

    // Initialize Nostr client
    let client = Client::new(&keys);

    // Add relays
    for relay_url in &config.nostr_relays {
        client.add_relay(relay_url).await?;
    }

    client.connect().await;
    log::info!("Connected to Nostr relays");

    // Initialize Blossom client
    let blossom_client = BlossomClient::new(keys.clone());

    // Initialize application state
    let state = Arc::new(AppState {
        seen_events: Arc::new(Mutex::new(HashSet::new())),
        client: client.clone(),
        blossom_client,
        config: config.clone(),
    });

    // Subscribe to DVM requests
    let filter = Filter::new()
        .kind(Kind::from(DVM_VIDEO_TRANSFORM_REQUEST_KIND))
        .since(Timestamp::now());

    client.subscribe(vec![filter], None).await?;
    log::info!("Subscribed to DVM video transform requests (kind {})", DVM_VIDEO_TRANSFORM_REQUEST_KIND);

    // Start cleanup task
    let cleanup_state = state.clone();
    tokio::spawn(async move {
        cleanup_task(cleanup_state).await;
    });

    // Main event loop
    let mut notifications = client.notifications();

    loop {
        tokio::select! {
            Ok(notification) = notifications.recv() => {
                if let RelayPoolNotification::Event { event, .. } = notification {
                    if event.kind == Kind::from(DVM_VIDEO_TRANSFORM_REQUEST_KIND) {
                        let state_clone = state.clone();
                        tokio::spawn(async move {
                            if let Err(e) = handle_request(state_clone, *event).await {
                                log::error!("Error handling request: {}", e);
                            }
                        });
                    }
                }
            }
        }
    }
}

async fn handle_request(state: Arc<AppState>, event: Event) -> Result<(), Box<dyn std::error::Error>> {
    // Check if we've seen this event before
    {
        let mut seen = state.seen_events.lock().await;
        if seen.contains(&event.id) {
            log::debug!("Skipping duplicate event: {}", event.id);
            return Ok(());
        }
        seen.insert(event.id);
    }

    log::info!("Received DVM request: {}", event.id);

    // Extract input URL
    let input_url = match get_input_url(&event) {
        Some(url) => url,
        None => {
            log::warn!("No input URL found in event {}", event.id);
            publish_error(&state, &event, "Missing input URL").await?;
            return Ok(());
        }
    };

    log::info!("Processing video from URL: {}", input_url);

    // Publish processing status
    publish_status(&state, &event, "processing", "Starting video processing").await?;

    // Get relay hints for publishing results
    let relay_hints = get_relay_hints(&event);
    let result_relays = if relay_hints.is_empty() {
        state.config.nostr_relays.clone()
    } else {
        relay_hints
    };

    // Process the video
    match process_job(&state, &input_url, &event).await {
        Ok(result_event) => {
            // Publish result
            log::info!("Publishing result for event {}", event.id);

            for relay_url in &result_relays {
                match state.client.send_event_to(vec![relay_url], result_event.clone()).await {
                    Ok(event_id) => log::info!("Published result to {}: {}", relay_url, event_id),
                    Err(e) => log::error!("Failed to publish to {}: {}", relay_url, e),
                }
            }

            publish_status(&state, &event, "success", "Video processing completed").await?;
        }
        Err(e) => {
            log::error!("Failed to process video: {}", e);
            publish_error(&state, &event, &format!("Processing failed: {}", e)).await?;
        }
    }

    Ok(())
}

async fn process_job(
    state: &AppState,
    input_url: &str,
    request_event: &Event,
) -> Result<Event, Box<dyn std::error::Error>> {
    // Create temporary directory for processing
    let temp_dir = tempfile::tempdir()?;
    let output_dir = temp_dir.path();

    log::info!("Processing in directory: {}", output_dir.display());

    // Process video with default config
    let config = TransformConfig::default();
    let processed = process_video(input_url, output_dir, &config).await?;

    // Make files content-addressable
    let hash_map = make_content_addressable(&processed).await?;

    // Upload all files to Blossom
    let mut result_tags = Vec::new();

    // Add request reference tags
    result_tags.push(Tag::event(request_event.id));
    result_tags.push(Tag::public_key(request_event.pubkey));

    // Add original input tag
    if let Some(input_tag) = request_event.tags.iter().find(|t| t.as_vec()[0] == "i") {
        result_tags.push(input_tag.clone());
    }

    // Add metadata tags
    result_tags.push(Tag::custom(
        TagKind::Custom("dim".into()),
        vec![format!("{}x{}", processed.metadata.width, processed.metadata.height)],
    ));
    result_tags.push(Tag::custom(
        TagKind::Custom("duration".into()),
        vec![processed.metadata.duration.round().to_string()],
    ));
    result_tags.push(Tag::custom(
        TagKind::Custom("size".into()),
        vec![processed.metadata.size.to_string()],
    ));

    // Upload master playlist
    log::info!("Uploading master playlist");
    let master_blob = state
        .blossom_client
        .upload_file(
            &state.config.blossom_upload_servers[0],
            &processed.master_playlist,
            &hash_map["master.m3u8"],
        )
        .await?;

    result_tags.push(Tag::custom(
        TagKind::Custom("master".into()),
        vec![master_blob.url.clone()],
    ));
    result_tags.push(Tag::custom(
        TagKind::Custom("x".into()),
        vec![master_blob.sha256.clone()],
    ));

    // Upload stream playlists
    let mut stream_files = Vec::new();
    let mut entries = tokio::fs::read_dir(output_dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.is_file() && path.extension().map(|e| e == "m3u8").unwrap_or(false) {
            if path.file_name().unwrap() != "master.m3u8" {
                stream_files.push(path);
            }
        }
    }

    for stream_path in stream_files {
        let hash = helpers::ffmpeg::calculate_sha256(&stream_path).await?;
        log::info!("Uploading stream playlist: {}", stream_path.display());

        let blob = state
            .blossom_client
            .upload_file(
                &state.config.blossom_upload_servers[0],
                &stream_path,
                &hash,
            )
            .await?;

        result_tags.push(Tag::custom(
            TagKind::Custom("stream".into()),
            vec![blob.url.clone()],
        ));
        result_tags.push(Tag::custom(
            TagKind::Custom("x".into()),
            vec![blob.sha256.clone()],
        ));
    }

    // Upload segments
    let mut segment_files = Vec::new();
    let mut entries = tokio::fs::read_dir(output_dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension() {
                if ext == "m4s" || ext == "ts" {
                    segment_files.push(path);
                }
            }
        }
    }

    for segment_path in segment_files {
        let hash = helpers::ffmpeg::calculate_sha256(&segment_path).await?;
        log::info!("Uploading segment: {}", segment_path.display());

        let blob = state
            .blossom_client
            .upload_file(
                &state.config.blossom_upload_servers[0],
                &segment_path,
                &hash,
            )
            .await?;

        result_tags.push(Tag::custom(
            TagKind::Custom("segment".into()),
            vec![blob.url.clone()],
        ));
        result_tags.push(Tag::custom(
            TagKind::Custom("x".into()),
            vec![blob.sha256.clone()],
        ));
    }

    // Build result event
    let result_event = EventBuilder::new(
        Kind::from(DVM_VIDEO_TRANSFORM_RESULT_KIND),
        "",
        result_tags,
    )
    .to_event(&state.client.keys().await?)?;

    Ok(result_event)
}

async fn publish_status(
    state: &AppState,
    request_event: &Event,
    status: &str,
    message: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let tags = vec![
        Tag::custom(TagKind::Custom("status".into()), vec![status.to_string()]),
        Tag::event(request_event.id),
        Tag::public_key(request_event.pubkey),
        Tag::custom(
            TagKind::Custom("expiration".into()),
            vec![(Timestamp::now().as_u64() + 3600).to_string()],
        ),
    ];

    let content = serde_json::json!({
        "msg": message
    })
    .to_string();

    let event = EventBuilder::new(Kind::from(DVM_STATUS_KIND), content, tags)
        .to_event(&state.client.keys().await?)?;

    state.client.send_event(event).await?;

    Ok(())
}

async fn publish_error(
    state: &AppState,
    request_event: &Event,
    error: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let tags = vec![
        Tag::custom(TagKind::Custom("status".into()), vec!["error".to_string()]),
        Tag::event(request_event.id),
        Tag::public_key(request_event.pubkey),
    ];

    let content = serde_json::json!({
        "error": error
    })
    .to_string();

    let event = EventBuilder::new(Kind::from(DVM_STATUS_KIND), content, tags)
        .to_event(&state.client.keys().await?)?;

    state.client.send_event(event).await?;

    Ok(())
}

async fn cleanup_task(state: Arc<AppState>) {
    let mut interval = time::interval(Duration::from_secs(3600)); // Run every hour

    loop {
        interval.tick().await;

        log::info!("Running blob cleanup task");

        for server in &state.config.blossom_upload_servers {
            match state
                .blossom_client
                .cleanup_old_blobs(server, state.config.blossom_blob_expiration_days)
                .await
            {
                Ok(count) => {
                    log::info!("Cleaned up {} blobs from {}", count, server);
                }
                Err(e) => {
                    log::error!("Failed to cleanup blobs from {}: {}", server, e);
                }
            }
        }
    }
}
