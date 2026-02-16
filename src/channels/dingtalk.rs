use super::dingtalk_api::{DingTalkApi, RobotMessage};
use super::dingtalk_stream::StreamClient;
use super::traits::{Channel, ChannelMessage};
use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::Mutex;

/// DingTalk channel using Stream mode (no public IP required)
pub struct DingTalkChannel {
    client_id: String,
    client_secret: String,
    allowed_users: Vec<String>,
    /// API client for sending messages
    api: Arc<Mutex<DingTalkApi>>,
}

impl DingTalkChannel {
    pub fn new(client_id: String, client_secret: String, allowed_users: Vec<String>) -> Self {
        Self {
            client_id: client_id.clone(),
            client_secret: client_secret.clone(),
            allowed_users,
            api: Arc::new(Mutex::new(DingTalkApi::new(client_id, client_secret))),
        }
    }

    fn is_user_allowed(&self, userid: &str) -> bool {
        self.allowed_users.iter().any(|u| u == "*" || u == userid)
    }
}

#[async_trait]
impl Channel for DingTalkChannel {
    fn name(&self) -> &str {
        "DingTalk"
    }

    async fn send(&self, message: &str, recipient: &str) -> Result<()> {
        // recipient format: "webhook_url"
        let api = self.api.lock().await;
        api.send_message_via_webhook(recipient, message).await?;
        tracing::info!("DingTalk message sent");
        Ok(())
    }

    async fn listen(&self, tx: tokio::sync::mpsc::Sender<ChannelMessage>) -> Result<()> {
        let mut stream_client =
            StreamClient::new(self.client_id.clone(), self.client_secret.clone());

        let allowed_users = self.allowed_users.clone();
        let api = self.api.clone();

        tracing::info!("DingTalk Stream client starting...");

        stream_client
            .connect(move |stream_msg| {
                let tx = tx.clone();
                let allowed_users = allowed_users.clone();
                let api = api.clone();

                tracing::info!("Processing Stream message, type: {}", stream_msg.msg_type);
                tracing::debug!("Stream message data: {}", stream_msg.data);

                // Parse robot message
                let robot_msg: RobotMessage = match serde_json::from_str(&stream_msg.data) {
                    Ok(m) => m,
                    Err(e) => {
                        tracing::error!("Failed to parse robot message: {e}, data: {}", stream_msg.data);
                        return Ok(DingTalkApi::build_ack(false));
                    }
                };

                // Check user permission first.
                let sender_id = robot_msg.sender_staff_id.clone();
                let is_allowed = allowed_users.iter().any(|u| u == "*" || u == &sender_id);
                if !is_allowed {
                    tracing::warn!("DingTalk message from unauthorized user: {sender_id}");
                    return Ok(DingTalkApi::build_ack(true)); // ACK but ignore
                }

                // Extract message content based on msgtype
                let content = match robot_msg.msg_type.as_str() {
                    "text" => {
                        if let Some(ref text) = robot_msg.text {
                            text.content.clone()
                        } else {
                            tracing::warn!("Text message without text field");
                            return Ok(DingTalkApi::build_ack(true));
                        }
                    }
                    "picture" => {
                        let webhook = robot_msg.session_webhook.clone();
                        let api = api.clone();
                        tokio::spawn(async move {
                            let hint = "已收到图片。当前 ZeroClaw 钉钉通道仅支持文本输入，暂不支持图片内容识别。请补充文字描述或关键信息。";
                            if let Err(e) = api.lock().await.send_message_via_webhook(&webhook, hint).await {
                                tracing::warn!("Failed to send picture unsupported hint: {e}");
                            }
                        });
                        return Ok(DingTalkApi::build_ack(true));
                    }
                    "audio" => {
                        let webhook = robot_msg.session_webhook.clone();
                        let api = api.clone();
                        tokio::spawn(async move {
                            let hint = "已收到语音。当前 ZeroClaw 钉钉通道仅支持文本输入，暂不支持语音识别。请改用文字发送。";
                            if let Err(e) = api.lock().await.send_message_via_webhook(&webhook, hint).await {
                                tracing::warn!("Failed to send audio unsupported hint: {e}");
                            }
                        });
                        return Ok(DingTalkApi::build_ack(true));
                    }
                    "video" => {
                        let webhook = robot_msg.session_webhook.clone();
                        let api = api.clone();
                        tokio::spawn(async move {
                            let hint = "已收到视频。当前 ZeroClaw 钉钉通道仅支持文本输入，暂不支持视频内容识别。请补充文字说明。";
                            if let Err(e) = api.lock().await.send_message_via_webhook(&webhook, hint).await {
                                tracing::warn!("Failed to send video unsupported hint: {e}");
                            }
                        });
                        return Ok(DingTalkApi::build_ack(true));
                    }
                    "file" => {
                        let webhook = robot_msg.session_webhook.clone();
                        let api = api.clone();
                        tokio::spawn(async move {
                            let hint = "已收到文件。当前 ZeroClaw 钉钉通道仅支持文本输入，暂不支持文件内容解析。请粘贴关键文本。";
                            if let Err(e) = api.lock().await.send_message_via_webhook(&webhook, hint).await {
                                tracing::warn!("Failed to send file unsupported hint: {e}");
                            }
                        });
                        return Ok(DingTalkApi::build_ack(true));
                    }
                    _ => {
                        tracing::warn!("Unsupported message type: {}", robot_msg.msg_type);
                        return Ok(DingTalkApi::build_ack(true));
                    }
                };

                tracing::info!("Received DingTalk message from user: {}, type: {}, content: {}",
                    robot_msg.sender_staff_id, robot_msg.msg_type, content);

                // Extract message content
                let webhook = robot_msg.session_webhook.clone();

                // Send to message bus
                // IMPORTANT: Use webhook as sender so replies can be sent back
                let channel_msg = ChannelMessage {
                    id: robot_msg.msg_id.clone(),
                    sender: webhook.clone(), // Use webhook URL as sender for reply
                    content: content.clone(),
                    channel: "DingTalk".to_string(),
                    timestamp: robot_msg.create_at as u64,
                };

                tracing::info!("Sending message to handler: user={}, webhook={}, content={}",
                    sender_id, webhook, content);

                // Use webhook as recipient for replies
                tokio::spawn(async move {
                    if let Err(e) = tx.send(channel_msg).await {
                        tracing::error!("Failed to send DingTalk message to handler: {e}");
                    } else {
                        tracing::info!("Message sent to handler successfully");
                    }
                });

                Ok(DingTalkApi::build_ack(true))
            })
            .await?;

        Ok(())
    }

    async fn health_check(&self) -> bool {
        // Try to get access token
        let mut api = self.api.lock().await;
        match api.get_access_token().await {
            Ok(_) => true,
            Err(e) => {
                tracing::warn!("DingTalk health check failed: {e}");
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_name() {
        let channel =
            DingTalkChannel::new("test_id".into(), "test_secret".into(), vec!["*".into()]);
        assert_eq!(channel.name(), "DingTalk");
    }

    #[test]
    fn test_user_allowed() {
        let channel = DingTalkChannel::new(
            "test_id".into(),
            "test_secret".into(),
            vec!["user1".into(), "user2".into()],
        );
        assert!(channel.is_user_allowed("user1"));
        assert!(channel.is_user_allowed("user2"));
        assert!(!channel.is_user_allowed("user3"));
    }

    #[test]
    fn test_wildcard_allowed() {
        let channel =
            DingTalkChannel::new("test_id".into(), "test_secret".into(), vec!["*".into()]);
        assert!(channel.is_user_allowed("anyone"));
    }

    #[tokio::test]
    async fn test_health_check() {
        let channel = DingTalkChannel::new(
            "invalid_id".into(),
            "invalid_secret".into(),
            vec!["*".into()],
        );
        // Should fail with invalid credentials
        assert!(!channel.health_check().await);
    }
}
