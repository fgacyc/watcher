use std::path::Path;

use hyper::StatusCode;
use tokio::io::{AsyncWriteExt, BufWriter};

use axum::{
    extract::{Host, Multipart, State},
    response::IntoResponse,
    Json,
};

use crate::{config::config, state::AppState};

/// A HTTP GET handler to get the current assets.
pub async fn assets_handler(Host(host): Host, State(state): State<AppState>) -> impl IntoResponse {
    Json(serde_json::json!({
        "assets": state.assets.read().await.iter().map(|(created, filename)| {
            serde_json::json!({"created": created, "filename": filename, "url": format!("https://{host}/{filename}")})
        }).collect::<Vec<_>>()
    }))
}

/// A HTTP POST handler to get the current assets.
pub async fn upload_handler(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    // Create uploads directory if it doesn't exist
    if let Err(err) = tokio::fs::create_dir_all(&config().assets_dir).await {
        eprintln!("Failed to create upload directory: {}", err);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to create upload directory",
        )
            .into_response();
    }

    // Process each field in the multipart form
    while let Some(field) = multipart.next_field().await.unwrap_or(None) {
        // Get the field name and filename
        let file_name = match field.file_name() {
            Some(name) => name.to_string(),
            None => continue,
        };

        // Make sure the file name is unique
        if state
            .assets
            .read()
            .await
            .iter()
            .any(|(_, filename)| filename == &file_name)
        {
            return (
                StatusCode::BAD_REQUEST,
                format!("File {file_name} already exists"),
            )
                .into_response();
        }

        // Generate a unique filename to prevent overwrites
        let file_path = Path::new(&config().assets_dir).join(&file_name);

        // Read the file data
        let data = match field.bytes().await {
            Ok(data) => data,
            Err(err) => {
                eprintln!("Failed to read file data: {}", err);
                return (StatusCode::BAD_REQUEST, "Failed to read file data").into_response();
            }
        };

        // Write the file to disk
        let file = match tokio::fs::File::create(&file_path).await {
            Ok(file) => file,
            Err(err) => {
                eprintln!("Failed to create file: {}", err);
                return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to save file").into_response();
            }
        };

        let mut writer = BufWriter::new(file);
        if let Err(err) = writer.write_all(&data).await {
            eprintln!("Failed to write file: {}", err);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to save file").into_response();
        }

        println!("Successfully saved file: {}", file_name);
        return (
            StatusCode::OK,
            format!("File uploaded successfully as {}", file_name),
        )
            .into_response();
    }

    (StatusCode::BAD_REQUEST, "No file provided").into_response()
}
