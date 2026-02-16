use super::dingtalk_api::{AckMessage, DingTalkApi, StreamMessage};
use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::{connect_async, tungstenite::Message};

/// DingTalk Stream WebSocket client
pub struct StreamClient {
    api: DingTalkApi,
}

impl StreamClient {
    pub fn new(client_id: String, client_secret: String) -> Self {
        Self {
            api: DingTalkApi::new(client_id, client_secret),
        }
    }

    /// Connect to DingTalk Stream and start listening
    pub async fn connect<F>(&mut self, mut callback: F) -> Result<()>
    where
        F: FnMut(StreamMessage) -> Result<AckMessage> + Send + 'static,
    {
        // 1. Open connection and get endpoint + ticket
        tracing::info!("Opening DingTalk Stream connection...");
        let conn = self.api.open_connection().await?;
        tracing::info!("Got endpoint: {}", conn.endpoint);

        // 2. Build WebSocket URL
        let ws_url = format!("{}?ticket={}", conn.endpoint, conn.ticket);

        // 3. Connect to WebSocket
        tracing::info!("Connecting to WebSocket...");
        let (ws_stream, _) = connect_async(&ws_url).await?;
        tracing::info!("WebSocket connected successfully");

        let (mut write, mut read) = ws_stream.split();

        // 4. Listen for messages
        while let Some(msg_result) = read.next().await {
            let msg = match msg_result {
                Ok(m) => m,
                Err(e) => {
                    tracing::error!("WebSocket error: {e}");
                    continue;
                }
            };

            match msg {
                Message::Text(text) => {
                    tracing::info!("Received WebSocket message: {}", text);

                    // Parse Stream message
                    let stream_msg: StreamMessage = match serde_json::from_str(&text) {
                        Ok(m) => m,
                        Err(e) => {
                            tracing::error!("Failed to parse message: {e}");
                            continue;
                        }
                    };

                    tracing::info!(
                        "Parsed message type: {}, topic: {:?}",
                        stream_msg.msg_type,
                        stream_msg.headers.topic
                    );

                    // Handle different message types
                    match stream_msg.msg_type.as_str() {
                        "SYSTEM" => {
                            // System message (ping/disconnect)
                            if let Some(topic) = &stream_msg.headers.topic {
                                if topic == "ping" {
                                    // Respond to ping - must return original data with opaque
                                    let response = serde_json::json!({
                                        "code": 200,
                                        "headers": {
                                            "messageId": stream_msg.headers.message_id,
                                            "contentType": "application/json"
                                        },
                                        "message": "OK",
                                        "data": stream_msg.data  // Return original data with opaque
                                    });
                                    tracing::debug!("Sending ping response: {}", response);
                                    if let Err(e) =
                                        write.send(Message::Text(response.to_string())).await
                                    {
                                        tracing::error!("Failed to send ping response: {e}");
                                    }
                                } else if topic == "disconnect" {
                                    tracing::info!(
                                        "Received disconnect message, closing connection"
                                    );
                                    break;
                                }
                            }
                        }
                        "CALLBACK" => {
                            // Robot message callback
                            tracing::info!("Received CALLBACK message");
                            let ack = callback(stream_msg.clone()).unwrap_or_else(|e| {
                                tracing::error!("Callback error: {e}");
                                AckMessage {
                                    code: 500,
                                    message: e.to_string(),
                                    data: None,
                                }
                            });

                            // Send ACK - must match protocol format
                            let ack_json = serde_json::json!({
                                "code": ack.code,
                                "headers": {
                                    "messageId": stream_msg.headers.message_id,
                                    "contentType": "application/json"
                                },
                                "message": ack.message,
                                "data": ack.data.unwrap_or_else(|| "{}".to_string())
                            });

                            tracing::debug!("Sending ACK: {}", ack_json);
                            if let Err(e) = write.send(Message::Text(ack_json.to_string())).await {
                                tracing::error!("Failed to send ACK: {e}");
                            }
                        }
                        _ => {
                            tracing::warn!("Unknown message type: {}", stream_msg.msg_type);
                        }
                    }
                }
                Message::Close(_) => {
                    tracing::info!("WebSocket connection closed");
                    break;
                }
                Message::Ping(data) => {
                    // Respond to WebSocket-level ping
                    if let Err(e) = write.send(Message::Pong(data)).await {
                        tracing::error!("Failed to send WebSocket pong: {e}");
                    }
                }
                _ => {}
            }
        }

        Ok(())
    }

    /// Get API reference for sending messages
    pub fn api(&self) -> &DingTalkApi {
        &self.api
    }

    pub fn api_mut(&mut self) -> &mut DingTalkApi {
        &mut self.api
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_creation() {
        let client = StreamClient::new("test_id".into(), "test_secret".into());
        assert_eq!(client.api.client_id, "test_id");
    }
}
